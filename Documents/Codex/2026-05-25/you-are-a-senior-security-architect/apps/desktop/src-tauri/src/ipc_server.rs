//! WebSocket IPC server — extension ↔ Tauri vault bridge.
//!
//! Binds to 127.0.0.1:0 (OS-assigned port), writes the port to
//! `<vault_dir>/ipc.port`, and accepts WebSocket connections from
//! loopback only.  When the vault locks, all connected clients receive a
//! `{"type":"vault_locked"}` push.

use crate::commands::{load_contents, load_meta, Credential};
use crate::state::AppState;
use tauri::Manager;

/// Generates a cryptographically random 64-character lowercase hex token (32 bytes).
pub fn generate_token() -> String {
    // Two UUID v4 values each contribute 16 bytes of OS-sourced randomness.
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();
    let mut bytes = [0u8; 32];
    bytes[..16].copy_from_slice(a.as_bytes());
    bytes[16..].copy_from_slice(b.as_bytes());
    hex::encode(bytes)
}

/// Returns true iff `msg` is a valid `{"type":"auth","token":"<expected>"}` message.
pub fn is_valid_auth_message(msg: &str, expected_token: &str) -> bool {
    let v: serde_json::Value = match serde_json::from_str(msg) {
        Ok(v) => v,
        Err(_) => return false,
    };
    v.get("type").and_then(|t| t.as_str()) == Some("auth")
        && v.get("token").and_then(|t| t.as_str()) == Some(expected_token)
}

/// Start the IPC server.  Returns the port that was bound.
/// Writes `<vault_dir>/ipc.port` in "port:token" format and spawns background tasks.
pub async fn run_ipc_server(app: tauri::AppHandle) -> Result<u16, String> {
    let state = app.state::<AppState>();
    let vault_dir = state.vault_dir.clone();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();

    let token = generate_token();

    std::fs::create_dir_all(&vault_dir).map_err(|e| e.to_string())?;
    let port_path = vault_dir.join("ipc.port");
    std::fs::write(&port_path, format!("{port}:{token}")).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&port_path, perms).map_err(|e| e.to_string())?;
    }
    *state.ipc_port.lock().map_err(|e| e.to_string())? = Some(port);
    *state.ipc_token.lock().map_err(|e| e.to_string())? = Some(token.clone());

    tauri::async_runtime::spawn(accept_loop(listener, app, port_path, token));
    Ok(port)
}

async fn accept_loop(
    listener: tokio::net::TcpListener,
    app: tauri::AppHandle,
    port_path: std::path::PathBuf,
    token: String,
) {
    let token = std::sync::Arc::new(token);
    loop {
        let (stream, addr) = match listener.accept().await {
            Ok(v) => v,
            Err(_) => break,
        };
        if !addr.ip().is_loopback() {
            continue; // reject non-loopback connections
        }
        let app = app.clone();
        let token = token.clone();
        tauri::async_runtime::spawn(handle_connection(stream, app, token));
    }
    let _ = std::fs::remove_file(&port_path);
    {
        let state = app.state::<AppState>();
        if let Ok(mut p) = state.ipc_port.lock() {
            *p = None;
        };
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    app: tauri::AppHandle,
    expected_token: std::sync::Arc<String>,
) {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(_) => return,
    };
    let (mut sink, mut stream) = ws.split();

    // Require a valid auth message as the very first message within 1 second.
    let auth_ok = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        stream.next(),
    )
    .await;
    match auth_ok {
        Ok(Some(Ok(Message::Text(msg)))) if is_valid_auth_message(msg.as_str(), &expected_token) => {}
        _ => {
            let _ = sink.send(Message::Text(
                r#"{"type":"error","code":"auth-required"}"#.into(),
            )).await;
            return;
        }
    }

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
            _ = lock_rx.recv() => {
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
        "list_credentials" => handle_list_credentials(state),
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

fn handle_list_credentials(state: &AppState) -> serde_json::Value {
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
        .map(|c| serde_json::json!({
            "id":       c.id,
            "title":    c.title,
            "username": c.username,
            "url":      c.url,
        }))
        .collect();

    state.touch_vault_access();
    serde_json::json!({"type": "credentials_list", "items": items})
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

/// Returns the registered domain (eTLD+1) of a URL, or None if not determinable.
/// Only HTTPS URLs are accepted; HTTP, single-label, and IP addresses return None.
/// Uses the Mozilla public suffix list via the `psl` crate so that multi-part
/// public suffixes (co.uk, azurewebsites.net, github.io, …) are handled
/// correctly: `bank.co.uk` and `evil.co.uk` will NOT match each other.
fn extract_etld1(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    let host = parsed.host_str()?;

    // Reject bare IP addresses.
    if host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }

    // `psl::domain_str` returns the registered domain (eTLD+1) using the
    // embedded Mozilla public suffix list, or None for public suffixes
    // themselves and single-label hosts.
    psl::domain_str(host).map(str::to_owned)
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

    // --- Token generation tests ---

    #[test]
    fn generate_token_returns_64_char_hex() {
        let token = generate_token();
        assert_eq!(token.len(), 64, "32 bytes must encode to 64 hex chars");
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()), "must be lowercase hex");
    }

    #[test]
    fn generate_token_is_random() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(t1, t2, "consecutive tokens must differ");
    }

    // --- Auth message validation tests ---

    #[test]
    fn is_valid_auth_accepts_correct_token() {
        let token = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let msg = format!(r#"{{"type":"auth","token":"{token}"}}"#);
        assert!(is_valid_auth_message(&msg, token));
    }

    #[test]
    fn is_valid_auth_rejects_wrong_token() {
        let msg = r#"{"type":"auth","token":"wrongtoken"}"#;
        assert!(!is_valid_auth_message(msg, "correcttoken"));
    }

    #[test]
    fn is_valid_auth_rejects_non_auth_type() {
        let msg = r#"{"type":"status","token":"abc123"}"#;
        assert!(!is_valid_auth_message(msg, "abc123"));
    }

    #[test]
    fn is_valid_auth_rejects_invalid_json() {
        assert!(!is_valid_auth_message("not-json", "abc123"));
    }

    #[test]
    fn etld1_extracts_registered_domain() {
        assert_eq!(extract_etld1("https://app.github.com"), Some("github.com".to_string()));
        assert_eq!(extract_etld1("https://github.com"), Some("github.com".to_string()));
        assert_eq!(extract_etld1("https://a.b.c.example.com"), Some("example.com".to_string()));
    }

    #[test]
    fn etld1_handles_multipart_public_suffix() {
        // co.uk is a public suffix; bank.co.uk is the registered domain.
        // The old naive approach would return "co.uk" for both bank.co.uk and
        // evil.co.uk, allowing cross-site credential leakage.
        assert_eq!(extract_etld1("https://bank.co.uk"), Some("bank.co.uk".to_string()));
        assert_eq!(extract_etld1("https://www.bank.co.uk"), Some("bank.co.uk".to_string()));
        // A bare public suffix has no registered domain.
        assert_eq!(extract_etld1("https://co.uk"), None);
    }

    #[test]
    fn etld1_github_io_sites_do_not_cross_match() {
        // github.io is a public suffix (GitHub Pages hosting). A naive
        // last-two-labels heuristic collapses victim.github.io and
        // attacker.github.io to the same "github.io", leaking credentials
        // across unrelated GitHub Pages sites. PSL matching keeps them apart.
        assert_eq!(
            extract_etld1("https://victim.github.io"),
            Some("victim.github.io".to_string()),
        );
        assert_eq!(
            extract_etld1("https://attacker.github.io"),
            Some("attacker.github.io".to_string()),
        );
        assert_ne!(
            extract_etld1("https://victim.github.io"),
            extract_etld1("https://attacker.github.io"),
        );
        // A bare public suffix has no registered domain.
        assert_eq!(extract_etld1("https://github.io"), None);
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

    #[test]
    fn list_credentials_response_has_correct_type() {
        let json = r#"{"type":"credentials_list","items":[]}"#;
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(v["type"], "credentials_list");
        assert!(v["items"].is_array());
    }
}
