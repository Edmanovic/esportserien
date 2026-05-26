//! WebSocket IPC server — extension ↔ Tauri vault bridge.
//!
//! Binds to 127.0.0.1:0 (OS-assigned port), writes the port to
//! `<vault_dir>/ipc.port`, and accepts WebSocket connections from
//! loopback only.  When the vault locks, all connected clients receive a
//! `{"type":"vault_locked"}` push.

use crate::commands::{load_contents, load_meta, Credential};
use crate::state::AppState;
use tauri::Manager;

/// Start the IPC server.  Returns the port that was bound.
/// Writes `<vault_dir>/ipc.port` and spawns background tasks.
pub async fn run_ipc_server(app: tauri::AppHandle) -> Result<u16, String> {
    let state = app.state::<AppState>();
    let vault_dir = state.vault_dir.clone();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();

    std::fs::create_dir_all(&vault_dir).map_err(|e| e.to_string())?;
    let port_path = vault_dir.join("ipc.port");
    std::fs::write(&port_path, port.to_string()).map_err(|e| e.to_string())?;
    *state.ipc_port.lock().map_err(|e| e.to_string())? = Some(port);

    tauri::async_runtime::spawn(accept_loop(listener, app, port_path));
    Ok(port)
}

async fn accept_loop(
    listener: tokio::net::TcpListener,
    app: tauri::AppHandle,
    port_path: std::path::PathBuf,
) {
    loop {
        let (stream, addr) = match listener.accept().await {
            Ok(v) => v,
            Err(_) => break,
        };
        if !addr.ip().is_loopback() {
            continue; // reject non-loopback connections
        }
        let app = app.clone();
        tauri::async_runtime::spawn(handle_connection(stream, app));
    }
    let _ = std::fs::remove_file(&port_path);
}

async fn handle_connection(stream: tokio::net::TcpStream, app: tauri::AppHandle) {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(_) => return,
    };
    let (mut sink, mut stream) = ws.split();
    let state = app.state::<AppState>();
    let mut lock_rx = state.lock_notify_tx.subscribe();

    loop {
        tokio::select! {
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let response = handle_message(text.as_str(), &state);
                        if sink.send(Message::Text(response.into())).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
            Ok(()) = lock_rx.recv() => {
                let _ = sink.send(Message::Text(
                    r#"{"type":"vault_locked"}"#.into()
                )).await;
                // Keep connection open — client may send unlock
            }
        }
    }
}

fn handle_message(text: &str, state: &AppState) -> String {
    let v: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return r#"{"type":"error","code":"invalid-json"}"#.to_string(),
    };

    let request_id = v.get("request_id").cloned();
    let msg_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let is_unlocked = state.secrets.lock().map_or(false, |s| s.is_unlocked());

    let mut response = match msg_type {
        "unlock" => handle_unlock(&v, state),
        "status" => {
            let vault_state = if is_unlocked { "unlocked" } else { "locked" };
            let autolock_minutes: Option<u32> = state
                .autolock_minutes
                .lock()
                .ok()
                .as_deref()
                .copied()
                .flatten();
            serde_json::json!({
                "type": "status",
                "vault_state": vault_state,
                "autolock_minutes": autolock_minutes,
            })
        }
        _ if !is_unlocked => serde_json::json!({"type": "error", "code": "vault-locked"}),
        "find_credentials" => handle_find_credentials(&v, state),
        "get_credential"   => handle_get_credential(&v, state),
        "lock" => {
            if let Ok(mut s) = state.secrets.lock() { s.lock(); }
            if let Ok(mut s) = state.session.lock() { *s = None; }
            let _ = state.lock_notify_tx.send(());
            serde_json::json!({"type": "lock_result", "ok": true})
        }
        _ => serde_json::json!({"type": "error", "code": "unknown-type"}),
    };

    if let Some(rid) = request_id {
        response["request_id"] = rid;
    }

    serde_json::to_string(&response)
        .unwrap_or_else(|_| r#"{"type":"error","code":"serialize-failed"}"#.to_string())
}

fn handle_unlock(v: &serde_json::Value, state: &AppState) -> serde_json::Value {
    let password = match v.get("password").and_then(|p| p.as_str()) {
        Some(p) => p.to_string(),
        None => return serde_json::json!({"type":"unlock_result","ok":false,"reason":"missing-password"}),
    };

    let meta = match load_meta(state) {
        Ok(m) => m,
        Err(_) => return serde_json::json!({"type":"unlock_result","ok":false,"reason":"vault-not-found"}),
    };

    use espass_crypto_core::SecureBuffer;
    let mut pw_buf = SecureBuffer::new(password.into_bytes());
    let now = time::OffsetDateTime::now_utc();

    let result = {
        let mut secrets = match state.secrets.lock() {
            Ok(s) => s,
            Err(_) => return serde_json::json!({"type":"unlock_result","ok":false,"reason":"state-error"}),
        };
        let mut um = match state.unlock_manager.lock() {
            Ok(u) => u,
            Err(_) => return serde_json::json!({"type":"unlock_result","ok":false,"reason":"state-error"}),
        };
        um.unlock(&mut pw_buf, &meta.salt, &meta.encrypted_vault_key, meta.vault_id, now, &mut secrets)
    };

    match result {
        Ok(session) => {
            if let Ok(mut s) = state.session.lock() { *s = Some(session); }
            state.touch_vault_access();
            serde_json::json!({"type":"unlock_result","ok":true})
        }
        Err(_) => serde_json::json!({"type":"unlock_result","ok":false,"reason":"wrong-password"}),
    }
}

fn handle_find_credentials(v: &serde_json::Value, state: &AppState) -> serde_json::Value {
    let origin = match v.get("origin").and_then(|o| o.as_str()) {
        Some(o) => o.to_string(),
        None => return serde_json::json!({"type":"error","code":"missing-origin"}),
    };

    let key_bytes = {
        let secrets = match state.secrets.lock() {
            Ok(s) => s,
            Err(_) => return serde_json::json!({"type":"error","code":"state-error"}),
        };
        let key = match secrets.vault_key() {
            Ok(k) => k,
            Err(_) => return serde_json::json!({"type":"error","code":"vault-locked"}),
        };
        let mut kb = [0u8; 32];
        kb.copy_from_slice(key.expose_secret());
        kb
    };
    let vault_key = espass_crypto_core::VaultKey::from_bytes(key_bytes);

    let contents = match load_contents(&vault_key, state) {
        Ok(c) => c,
        Err(e) => return serde_json::json!({"type":"error","code":"load-failed","message":e}),
    };

    let items: Vec<serde_json::Value> = contents
        .credentials
        .iter()
        .filter(|c| credential_matches_origin(c, &origin))
        .map(|c| serde_json::json!({"id": c.id, "title": c.title, "username": c.username}))
        .collect();

    state.touch_vault_access();
    serde_json::json!({"type": "credentials", "items": items})
}

fn handle_get_credential(v: &serde_json::Value, state: &AppState) -> serde_json::Value {
    let id = match v.get("id").and_then(|i| i.as_str()) {
        Some(i) => i.to_string(),
        None => return serde_json::json!({"type":"error","code":"missing-id"}),
    };

    let key_bytes = {
        let secrets = match state.secrets.lock() {
            Ok(s) => s,
            Err(_) => return serde_json::json!({"type":"error","code":"state-error"}),
        };
        let key = match secrets.vault_key() {
            Ok(k) => k,
            Err(_) => return serde_json::json!({"type":"error","code":"vault-locked"}),
        };
        let mut kb = [0u8; 32];
        kb.copy_from_slice(key.expose_secret());
        kb
    };
    let vault_key = espass_crypto_core::VaultKey::from_bytes(key_bytes);

    let contents = match load_contents(&vault_key, state) {
        Ok(c) => c,
        Err(e) => return serde_json::json!({"type":"error","code":"load-failed","message":e}),
    };

    match contents.credentials.iter().find(|c| c.id == id) {
        Some(cred) => {
            state.touch_vault_access();
            serde_json::json!({
                "type": "credential",
                "username": cred.username,
                "password": cred.password,
            })
        }
        None => serde_json::json!({"type":"error","code":"not-found"}),
    }
}

// ---------------------------------------------------------------------------
// eTLD+1 matching
// ---------------------------------------------------------------------------

/// Extract the effective TLD+1 from an HTTPS URL.
/// Returns `None` for HTTP URLs or malformed/single-label hosts.
fn extract_etld1(url: &str) -> Option<String> {
    if !url.starts_with("https://") {
        return None;
    }
    let rest = url.strip_prefix("https://")?;
    let host = rest.split('/').next()?.split(':').next()?;
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    Some(format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]))
}

fn credential_matches_origin(cred: &Credential, origin: &str) -> bool {
    let url = match cred.url.as_deref() {
        Some(u) => u,
        None => return false,
    };
    let req = match extract_etld1(origin) {
        Some(e) => e,
        None => return false,
    };
    let cred_etld1 = match extract_etld1(url) {
        Some(e) => e,
        None => return false,
    };
    req == cred_etld1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etld1_https_extracts_last_two_labels() {
        assert_eq!(extract_etld1("https://app.github.com"), Some("github.com".to_string()));
        assert_eq!(extract_etld1("https://github.com"), Some("github.com".to_string()));
        assert_eq!(extract_etld1("https://a.b.c.example.com"), Some("example.com".to_string()));
    }

    #[test]
    fn etld1_rejects_http() {
        assert_eq!(extract_etld1("http://github.com"), None);
    }

    #[test]
    fn etld1_rejects_single_label() {
        assert_eq!(extract_etld1("https://localhost"), None);
    }

    #[test]
    fn credential_matches_same_etld1() {
        let cred = crate::commands::Credential {
            id: "1".into(), title: "GitHub".into(), username: "u".into(),
            password: "p".into(), url: Some("https://github.com".into()),
            created_at: 0, updated_at: 0,
        };
        assert!(credential_matches_origin(&cred, "https://app.github.com"));
        assert!(credential_matches_origin(&cred, "https://github.com"));
    }

    #[test]
    fn credential_rejects_different_domain() {
        let cred = crate::commands::Credential {
            id: "1".into(), title: "GitHub".into(), username: "u".into(),
            password: "p".into(), url: Some("https://github.com".into()),
            created_at: 0, updated_at: 0,
        };
        assert!(!credential_matches_origin(&cred, "https://evil.com"));
        assert!(!credential_matches_origin(&cred, "http://github.com"));
    }
}
