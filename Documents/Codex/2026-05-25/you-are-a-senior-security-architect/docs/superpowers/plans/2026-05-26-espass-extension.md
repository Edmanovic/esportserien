# ESPASS Browser Extension Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Chrome/Chromium browser extension that detects login forms, shows a Shadow DOM credential dropdown, and fills username/password fields — communicating with the Tauri desktop app via a native messaging host stdio↔WebSocket bridge, with system tray mode and configurable auto-lock.

**Architecture:** The Tauri desktop app runs in the system tray hosting a local WebSocket IPC server (`ipc_server.rs`). A small `espass-host` binary (spawned by Chrome) bridges Chrome's native messaging stdio to that WebSocket. The Chrome extension (MV3 service worker + content script + popup) talks to the background via Chrome messaging; the background routes to the native host. Credentials never leave the Tauri process until the user selects one. The IPC server runs continuously — even when the vault is locked — so the extension can send `unlock` messages.

**Tech Stack:** Rust / Tauri v2 (add `tokio`, `tokio-tungstenite`, `futures-util`, `tauri-plugin-autostart`); TypeScript bundled with `esbuild` (no runtime framework); native messaging host is the existing workspace Cargo member (rewritten).

---

## File Map

| File | Role |
|------|------|
| `apps/desktop/src-tauri/Cargo.toml` | Add `tokio`, `tokio-tungstenite`, `futures-util`, `tauri-plugin-autostart`; enable `tray-icon` feature |
| `apps/desktop/src-tauri/src/state.rs` | Add IPC fields to `AppState`: `lock_notify_tx`, `autolock_minutes`, `last_vault_access`, `ipc_port` |
| `apps/desktop/src-tauri/src/commands.rs` | Make `load_contents`, `load_meta`, `Credential`, `VaultContents` `pub(crate)`; add `set_autolock_timeout`, `get_lock_status`; add `touch_vault_access` calls |
| `apps/desktop/src-tauri/src/ipc_server.rs` | New — WebSocket IPC server; handles `find_credentials`, `get_credential`, `unlock`, `lock`, `status`; pushes `vault_locked` |
| `apps/desktop/src-tauri/src/tray.rs` | New — system tray icon, autolock timer task, autostart plugin |
| `apps/desktop/src-tauri/src/lib.rs` | Wire tray, IPC server, new commands |
| `apps/desktop/native-messaging-host/src/main.rs` | Replace — simple async stdio↔WebSocket bridge |
| `apps/desktop/native-messaging-host/Cargo.toml` | Replace — `tokio`, `tokio-tungstenite`, `futures-util`, `serde_json` only |
| `apps/extension/manifest.chrome.json` | Add `action` field for popup |
| `apps/extension/manifest.json` | New — dist-based manifest for loading packed extension |
| `apps/extension/src/background/service-worker.ts` | Rewrite — persistent native port, response routing by request_id, credential cache, content port tracking |
| `apps/extension/src/content/autofill-guard.ts` | Rewrite — only trigger on password fields; background port; dispatch dropdown; fill fields |
| `apps/extension/src/content/dropdown.ts` | New — Shadow DOM dropdown component |
| `apps/extension/src/popup/popup.html` | New — popup HTML shell (3 states) |
| `apps/extension/src/popup/popup.ts` | New — popup state logic and unlock form |
| `package.json` | Add `esbuild` devDependency and `extension:build` script |

---

### Task 1: Desktop Cargo.toml + AppState IPC additions

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/src/state.rs`

- [ ] **Step 1: Write a failing test for `touch_vault_access`**

Add at the bottom of `apps/desktop/src-tauri/src/state.rs`:

```rust
#[cfg(test)]
mod ipc_state_tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn touch_vault_access_updates_timestamp() {
        let state = AppState::new(std::path::PathBuf::from("test-vault-ipc"));
        let before = state.last_vault_access.load(Ordering::Relaxed);
        // Before any touch, timestamp is 0
        assert_eq!(before, 0);
        state.touch_vault_access();
        let after = state.last_vault_access.load(Ordering::Relaxed);
        assert!(after > 0, "timestamp should be set after touch");
    }
}
```

- [ ] **Step 2: Run test — expect FAIL (fields not defined yet)**

```
cargo test -p espass-desktop touch_vault_access_updates_timestamp
```

Expected: compile error — `touch_vault_access`, `last_vault_access` not found.

- [ ] **Step 3: Add dependencies to `apps/desktop/src-tauri/Cargo.toml`**

The sync plan already added `reqwest`, `hkdf`, `sha2`, `hex`, `base64`, `espass-shared-types`. Add the IPC/tray additions. The complete updated `[dependencies]` section:

```toml
[dependencies]
espass-crypto-core  = { version = "0.1.0", path = "../../../packages/crypto-core" }
espass-vault-runtime = { version = "0.1.0", path = "../../../packages/vault-runtime" }
espass-shared-types = { version = "0.1.0", path = "../../../packages/shared-types" }
serde       = { version = "1.0.203", features = ["derive"] }
serde_json  = "1.0.117"
csv         = "1.3"
tauri-plugin-dialog    = "2"
tauri-plugin-autostart = "2"
tauri = { version = "2.0.0", features = ["tray-icon"] }
thiserror   = "1.0.61"
time        = { version = "0.3.36", features = ["serde"] }
uuid        = { version = "1.8.0", features = ["serde", "v4"] }
zeroize     = { version = "1.8.1", features = ["derive"] }
hkdf        = "0.12"
sha2        = "0.10"
hex         = "0.4"
base64      = "0.22"
reqwest     = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
tokio           = { version = "1", features = ["rt-multi-thread", "net", "time", "sync", "macros"] }
tokio-tungstenite = "0.23"
futures-util    = "0.3"
```

- [ ] **Step 4: Update `apps/desktop/src-tauri/src/state.rs`**

Add the IPC fields and `touch_vault_access` method. The full updated file (preserving existing sync additions from the sync plan):

```rust
//! Shared Tauri application state.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, Mutex};

use espass_crypto_core::{EncryptedEnvelope, KdfParams, Salt};
use espass_vault_runtime::{RuntimeSecretStore, SessionRuntime, UnlockManager};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Vault metadata stored on disk (unencrypted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultMeta {
    pub vault_id: Uuid,
    pub kdf_params: KdfParams,
    pub salt: Salt,
    pub encrypted_vault_key: EncryptedEnvelope,
    pub data_revision: u64,
}

// ---------------------------------------------------------------------------
// Sync state (added by sync plan)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemSyncRecord {
    pub server_revision: u64,
    pub last_pushed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStateFile {
    pub server_url: String,
    pub user_id: String,
    pub vault_id: String,
    pub last_synced_at: Option<i64>,
    #[serde(default)]
    pub pending_deletes: Vec<String>,
    #[serde(default)]
    pub items: HashMap<String, ItemSyncRecord>,
}

pub struct SyncState {
    pub server_url: String,
    pub user_id: Uuid,
    pub vault_id: Uuid,
    pub jwt: String,
    pub refresh_token: String,
    pub jwt_expires_at: i64,
    pub last_synced_at: Option<i64>,
    pub status: SyncStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SyncStatus {
    NotConfigured,
    Idle { last_synced_at: i64 },
    Syncing,
    Error { message: String },
    Unauthenticated,
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

/// Tauri-managed application state.
pub struct AppState {
    pub secrets: Mutex<RuntimeSecretStore>,
    pub session: Mutex<Option<SessionRuntime>>,
    pub unlock_manager: Mutex<UnlockManager>,
    pub vault_dir: PathBuf,
    // Sync (added by sync plan)
    pub sync: Mutex<Option<SyncState>>,
    // IPC / tray
    /// Broadcast sender — send `()` to notify all WebSocket clients the vault locked.
    pub lock_notify_tx: tokio::sync::broadcast::Sender<()>,
    /// Configured auto-lock duration. `None` = never.
    pub autolock_minutes: Mutex<Option<u32>>,
    /// Unix timestamp of the last vault access (find/get/unlock). Used by auto-lock timer.
    pub last_vault_access: Arc<AtomicI64>,
    /// Port the IPC WebSocket server is listening on (set after startup).
    pub ipc_port: Mutex<Option<u16>>,
}

impl Default for AppState {
    fn default() -> Self {
        let vault_dir = std::env::var("APPDATA")
            .or_else(|_| std::env::var("XDG_DATA_HOME"))
            .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.local/share")))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("espass");
        Self::new(vault_dir)
    }
}

impl AppState {
    #[must_use]
    pub fn new(vault_dir: PathBuf) -> Self {
        let (lock_notify_tx, _) = tokio::sync::broadcast::channel(16);
        Self {
            secrets: Mutex::new(RuntimeSecretStore::locked()),
            session: Mutex::new(None),
            unlock_manager: Mutex::new(UnlockManager::new(KdfParams::default())),
            vault_dir,
            sync: Mutex::new(None),
            lock_notify_tx,
            autolock_minutes: Mutex::new(Some(15)),
            last_vault_access: Arc::new(AtomicI64::new(0)),
            ipc_port: Mutex::new(None),
        }
    }

    #[must_use]
    pub fn meta_path(&self) -> PathBuf { self.vault_dir.join("vault.meta.json") }

    #[must_use]
    pub fn data_path(&self) -> PathBuf { self.vault_dir.join("vault.data.json") }

    #[must_use]
    pub fn sync_state_path(&self) -> PathBuf { self.vault_dir.join("sync_state.json") }

    #[must_use]
    pub fn vault_exists(&self) -> bool { self.meta_path().exists() }

    /// Records that the vault was accessed right now; resets the auto-lock timer.
    pub fn touch_vault_access(&self) {
        use std::sync::atomic::Ordering;
        self.last_vault_access.store(
            time::OffsetDateTime::now_utc().unix_timestamp(),
            Ordering::Relaxed,
        );
    }
}

#[cfg(test)]
mod ipc_state_tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn touch_vault_access_updates_timestamp() {
        let state = AppState::new(std::path::PathBuf::from("test-vault-ipc"));
        let before = state.last_vault_access.load(Ordering::Relaxed);
        assert_eq!(before, 0);
        state.touch_vault_access();
        let after = state.last_vault_access.load(Ordering::Relaxed);
        assert!(after > 0, "timestamp should be set after touch");
    }
}
```

- [ ] **Step 5: Run test — expect PASS**

```
cargo test -p espass-desktop touch_vault_access_updates_timestamp
```

Expected: `test ipc_state_tests::touch_vault_access_updates_timestamp ... ok`

- [ ] **Step 6: Commit**

```
git add apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/src/state.rs
git commit -m "feat(desktop): add IPC state fields and touch_vault_access to AppState"
```

---

### Task 2: Desktop WebSocket IPC server (`ipc_server.rs`)

**Files:**
- Create: `apps/desktop/src-tauri/src/ipc_server.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs` (make helpers `pub(crate)`)

- [ ] **Step 1: Write failing tests for eTLD+1 matching**

Create `apps/desktop/src-tauri/src/ipc_server.rs` with just the tests:

```rust
//! WebSocket IPC server — extension ↔ Tauri vault bridge.

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
```

- [ ] **Step 2: Run test — expect FAIL**

```
cargo test -p espass-desktop etld1
```

Expected: compile error — functions not defined yet.

- [ ] **Step 3: Make `Credential`, `load_contents`, `load_meta` accessible from `ipc_server.rs`**

In `apps/desktop/src-tauri/src/commands.rs`, change visibility of the shared types and helpers. Find these four items and add `pub(crate)`:

```rust
// Change:  pub struct Credential {
// To:
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct Credential { ... }  // keep fields unchanged

// Change:  pub struct VaultContents {
// To:
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub(crate) struct VaultContents { ... }

// Change:  fn load_meta(
// To:
pub(crate) fn load_meta(state: &AppState) -> Result<VaultMeta, String> { ... }

// Change:  fn load_contents(
// To:
pub(crate) fn load_contents(
    key: &espass_crypto_core::VaultKey,
    state: &AppState,
) -> Result<VaultContents, String> { ... }
```

Also add `touch_vault_access` calls to vault-reading commands. In `list_credentials`, `get_credential`, `add_credential`, `update_credential`, `delete_credential`, add this line before returning `Ok(...)`:

```rust
state.touch_vault_access();
```

- [ ] **Step 4: Write the full `ipc_server.rs`**

```rust
//! WebSocket IPC server — extension ↔ Tauri vault bridge.
//!
//! Binds to 127.0.0.1:0 (OS-assigned port), writes the port to
//! `%APPDATA%/espass/ipc.port`, and accepts WebSocket connections from
//! loopback only.  When the vault locks, all connected clients receive a
//! `{"type":"vault_locked"}` push.

use crate::commands::{load_contents, load_meta, Credential};
use crate::state::AppState;

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
            let autolock = *state.autolock_minutes.lock().map_or_else(|_| std::sync::MutexGuard::leak(state.autolock_minutes.lock().unwrap()), |g| g);
            // Simpler approach — just read the lock:
            let autolock_minutes: Option<u32> = state.autolock_minutes.lock()
                .ok()
                .and_then(|g| *g);
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
            state.secrets.lock().map(|mut s| s.lock()).ok();
            *state.session.lock().map_err(|_| ()).unwrap() = None;
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

    let (key_bytes,) = {
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
        (kb,)
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

    let (key_bytes,) = {
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
        (kb,)
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
```

Note: the `status` handler has a small borrow issue — simplify it to:

```rust
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
```

- [ ] **Step 5: Run tests — expect PASS**

```
cargo test -p espass-desktop etld1 credential_matches
```

Expected: 4 tests pass.

- [ ] **Step 6: Commit**

```
git add apps/desktop/src-tauri/src/ipc_server.rs apps/desktop/src-tauri/src/commands.rs
git commit -m "feat(desktop): add WebSocket IPC server with credential matching"
```

---

### Task 3: System tray + autolock (`tray.rs`)

**Files:**
- Create: `apps/desktop/src-tauri/src/tray.rs`

- [ ] **Step 1: Write a failing test for the autolock timing logic**

Create `apps/desktop/src-tauri/src/tray.rs` with just the tests:

```rust
//! System tray, autostart registration, and auto-lock timer.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_autolock_when_threshold_exceeded() {
        assert!(should_lock(100, 1000, 900));
    }

    #[test]
    fn should_not_autolock_when_recently_accessed() {
        assert!(!should_lock(950, 1000, 900));
    }

    #[test]
    fn exact_boundary_triggers_lock() {
        assert!(should_lock(100, 1000, 900));  // 1000 - 100 = 900 >= 900
    }
}
```

- [ ] **Step 2: Run test — expect FAIL**

```
cargo test -p espass-desktop should_autolock
```

Expected: compile error — `should_lock` not defined.

- [ ] **Step 3: Implement `tray.rs`**

```rust
//! System tray, autostart registration, and auto-lock timer.

use crate::state::AppState;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    tray::TrayIconBuilder,
    Manager,
};

/// Returns true when enough time has passed since the last vault access.
///
/// `last_access` and `now` are Unix timestamps (seconds).
/// `threshold_secs` is the configured lock delay.
pub(crate) fn should_lock(last_access: i64, now: i64, threshold_secs: i64) -> bool {
    now - last_access >= threshold_secs
}

/// Build and register the system tray icon and menu.
pub fn setup_tray(app: &tauri::App) -> Result<(), tauri::Error> {
    let open_item  = MenuItemBuilder::new("Åbn").id("open").build(app)?;
    let lock_item  = MenuItemBuilder::new("Lås vault").id("lock").build(app)?;
    let sep1       = PredefinedMenuItem::separator(app)?;

    let al1   = MenuItemBuilder::new("1 min").id("al_1").build(app)?;
    let al5   = MenuItemBuilder::new("5 min").id("al_5").build(app)?;
    let al15  = MenuItemBuilder::new("15 min (standard)").id("al_15").build(app)?;
    let al60  = MenuItemBuilder::new("60 min").id("al_60").build(app)?;
    let al240 = MenuItemBuilder::new("240 min").id("al_240").build(app)?;
    let al_never = MenuItemBuilder::new("Aldrig").id("al_never").build(app)?;

    let al_sub = SubmenuBuilder::new(app, "Auto-lock")
        .item(&al1).item(&al5).item(&al15)
        .item(&al60).item(&al240).item(&al_never)
        .build()?;

    let sep2      = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItemBuilder::new("Afslut").id("quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&open_item)
        .item(&lock_item)
        .item(&sep1)
        .item(&al_sub)
        .item(&sep2)
        .item(&quit_item)
        .build()?;

    TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("ESPASS")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| handle_menu_event(app, event.id().as_ref()))
        .build(app)?;

    Ok(())
}

fn handle_menu_event(app: &tauri::AppHandle, id: &str) {
    match id {
        "open" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        "lock" => {
            let state = app.state::<AppState>();
            if let Ok(mut s) = state.secrets.lock() { s.lock(); }
            if let Ok(mut s) = state.session.lock() { *s = None; }
            let _ = state.lock_notify_tx.send(());
        }
        "quit" => std::process::exit(0),
        al if al.starts_with("al_") => {
            let minutes: Option<u32> = match al {
                "al_1"    => Some(1),
                "al_5"    => Some(5),
                "al_15"   => Some(15),
                "al_60"   => Some(60),
                "al_240"  => Some(240),
                "al_never" => None,
                _ => return,
            };
            let state = app.state::<AppState>();
            if let Ok(mut m) = state.autolock_minutes.lock() { *m = minutes; }
        }
        _ => {}
    }
}

/// Spawn the background auto-lock timer task.
/// Wakes every 30 seconds and locks the vault if the configured timeout has elapsed.
pub fn start_autolock_task(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        interval.tick().await; // skip immediate first tick
        loop {
            interval.tick().await;
            let state = app.state::<AppState>();

            let is_unlocked = state.secrets.lock().map_or(false, |s| s.is_unlocked());
            if !is_unlocked {
                continue;
            }

            let threshold_secs: i64 = {
                let minutes = state.autolock_minutes.lock().ok().as_deref().copied().flatten();
                match minutes {
                    None => continue, // never auto-lock
                    Some(m) => m as i64 * 60,
                }
            };

            let last_access = state
                .last_vault_access
                .load(std::sync::atomic::Ordering::Relaxed);
            let now = time::OffsetDateTime::now_utc().unix_timestamp();

            if should_lock(last_access, now, threshold_secs) {
                if let Ok(mut s) = state.secrets.lock() { s.lock(); }
                if let Ok(mut s) = state.session.lock() { *s = None; }
                let _ = state.lock_notify_tx.send(());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_autolock_when_threshold_exceeded() {
        assert!(should_lock(100, 1000, 900));
    }

    #[test]
    fn should_not_autolock_when_recently_accessed() {
        assert!(!should_lock(950, 1000, 900));
    }

    #[test]
    fn exact_boundary_triggers_lock() {
        assert!(should_lock(100, 1000, 900));
    }
}
```

- [ ] **Step 4: Run tests — expect PASS**

```
cargo test -p espass-desktop should_autolock should_not_autolock exact_boundary
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```
git add apps/desktop/src-tauri/src/tray.rs
git commit -m "feat(desktop): system tray, auto-lock timer, autostart"
```

---

### Task 4: New commands + `lib.rs` wiring

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing test for `get_lock_status`**

Add to the bottom of `commands.rs`:

```rust
#[cfg(test)]
mod lock_cmd_tests {
    use super::*;

    #[test]
    fn lock_status_default_values() {
        let state = crate::AppState::new(std::path::PathBuf::from("test-lock-cmd"));
        let status = LockStatus {
            unlocked: state.secrets.lock().unwrap().is_unlocked(),
            autolock_minutes: *state.autolock_minutes.lock().unwrap(),
            ipc_port: *state.ipc_port.lock().unwrap(),
        };
        assert!(!status.unlocked);
        assert_eq!(status.autolock_minutes, Some(15));
        assert_eq!(status.ipc_port, None);
    }
}
```

- [ ] **Step 2: Run test — expect FAIL**

```
cargo test -p espass-desktop lock_status_default_values
```

Expected: compile error — `LockStatus` not defined yet.

- [ ] **Step 3: Add `set_autolock_timeout` and `get_lock_status` to `commands.rs`**

Append after the existing import/export commands:

```rust
// ---------------------------------------------------------------------------
// IPC / Tray commands
// ---------------------------------------------------------------------------

/// Sets the auto-lock timeout.  `None` = never lock automatically.
#[tauri::command]
pub fn set_autolock_timeout(minutes: Option<u32>, state: State<AppState>) -> Result<(), String> {
    *state.autolock_minutes.lock().map_err(|e| e.to_string())? = minutes;
    Ok(())
}

/// Returns the current lock status, auto-lock setting, and IPC port.
#[derive(serde::Serialize)]
pub struct LockStatus {
    pub unlocked: bool,
    pub autolock_minutes: Option<u32>,
    pub ipc_port: Option<u16>,
}

#[tauri::command]
pub fn get_lock_status(state: State<AppState>) -> Result<LockStatus, String> {
    let unlocked = state.secrets.lock().map_err(|e| e.to_string())?.is_unlocked();
    let autolock_minutes = *state.autolock_minutes.lock().map_err(|e| e.to_string())?;
    let ipc_port = *state.ipc_port.lock().map_err(|e| e.to_string())?;
    Ok(LockStatus { unlocked, autolock_minutes, ipc_port })
}
```

- [ ] **Step 4: Run test — expect PASS**

```
cargo test -p espass-desktop lock_status_default_values
```

Expected: `test lock_cmd_tests::lock_status_default_values ... ok`

- [ ] **Step 5: Rewrite `lib.rs`**

```rust
//! ESPASS desktop application library entry point.

mod commands;
mod ipc_server;
mod state;
mod sync;
mod tray;

pub use state::AppState;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(state::AppState::default())
        .setup(|app| {
            // Register this app to start with Windows/macOS login.
            use tauri_plugin_autostart::ManagerExt;
            let _ = app.autolaunch().enable();

            // Start WebSocket IPC server (extension bridge).
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = ipc_server::run_ipc_server(handle).await {
                    eprintln!("[espass] IPC server error: {e}");
                }
            });

            // System tray.
            tray::setup_tray(app)?;

            // Auto-lock background timer.
            tray::start_autolock_task(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_exists,
            commands::create_vault,
            commands::unlock_vault,
            commands::lock_vault,
            commands::get_session_status,
            commands::list_credentials,
            commands::add_credential,
            commands::get_credential,
            commands::delete_credential,
            commands::update_credential,
            commands::generate_password,
            commands::import_credentials,
            commands::import_credentials_json,
            commands::export_credentials_csv,
            commands::export_credentials_json,
            commands::set_autolock_timeout,
            commands::get_lock_status,
            commands::sync_configure,
            commands::sync_now_cmd,
            commands::get_sync_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 6: Build check**

```
cargo build -p espass-desktop
```

Expected: compiles without errors.

- [ ] **Step 7: Commit**

```
git add apps/desktop/src-tauri/src/commands.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): add set_autolock_timeout, get_lock_status; wire tray and IPC into app startup"
```

---

### Task 5: Native messaging host rewrite

**Files:**
- Replace: `apps/desktop/native-messaging-host/Cargo.toml`
- Replace: `apps/desktop/native-messaging-host/src/main.rs`

The existing host has a complex IPC session handshake protocol. We replace it with a simple stdio↔WebSocket bridge — the host has no vault logic and stores no secrets.

- [ ] **Step 1: Write failing test for the message framing functions**

Replace `apps/desktop/native-messaging-host/src/main.rs` with just the skeleton + tests:

```rust
//! ESPASS native messaging host — stdio ↔ WebSocket bridge.

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn native_msg_round_trip() {
        let text = r#"{"type":"ping","request_id":"abc-123"}"#;
        let mut buf: Vec<u8> = Vec::new();
        write_native_msg(&mut buf, text).await.unwrap();
        let mut cursor = std::io::Cursor::new(buf);
        let decoded = read_native_msg(&mut cursor).await.unwrap();
        assert_eq!(decoded, Some(text.to_string()));
    }

    #[tokio::test]
    async fn empty_stdin_returns_none() {
        let mut cursor = std::io::Cursor::new(vec![]);
        let result = read_native_msg(&mut cursor).await.unwrap();
        assert_eq!(result, None);
    }
}
```

- [ ] **Step 2: Run test — expect FAIL**

```
cargo test -p espass-native-messaging-host
```

Expected: compile error — `write_native_msg`, `read_native_msg` not defined.

- [ ] **Step 3: Replace `Cargo.toml`**

```toml
[package]
name = "espass-native-messaging-host"
version = "0.1.0"
description = "Native messaging host for ESPASS desktop-extension IPC."
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true

[[bin]]
name = "espass-host"
path = "src/main.rs"

[dependencies]
tokio        = { version = "1", features = ["rt-multi-thread", "io-std", "net"] }
tokio-tungstenite = "0.23"
futures-util = "0.3"
serde_json   = "1"

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "io-std", "net", "macros"] }

[lints]
workspace = true
```

- [ ] **Step 4: Write the full `main.rs`**

```rust
//! ESPASS native messaging host — stdio ↔ WebSocket bridge.
//!
//! Chrome spawns this binary and communicates over stdin/stdout using the
//! native messaging length-prefix protocol (4-byte LE u32 + UTF-8 JSON).
//! This host reads `%APPDATA%\espass\ipc.port` and connects to the
//! Tauri desktop app's local WebSocket IPC server, then forwards messages
//! in both directions until stdin closes or the WebSocket disconnects.

#![allow(clippy::expect_used)]

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() {
    if let Err(code) = run().await {
        // Send a structured error to the browser so the extension can react.
        let msg = serde_json::to_vec(&serde_json::json!({
            "type": "error",
            "code": code,
        }))
        .unwrap_or_default();
        let mut stdout = tokio::io::stdout();
        let _ = stdout.write_all(&(msg.len() as u32).to_le_bytes()).await;
        let _ = stdout.write_all(&msg).await;
        let _ = stdout.flush().await;
        std::process::exit(1);
    }
}

async fn run() -> Result<(), &'static str> {
    let port = read_ipc_port()?;
    let url = format!("ws://127.0.0.1:{port}");

    let (ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|_| "desktop-unavailable")?;

    let (mut ws_sink, mut ws_stream) = ws.split();
    let mut stdin  = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    loop {
        tokio::select! {
            // stdin (Chrome) → WebSocket (Tauri)
            result = read_native_msg(&mut stdin) => {
                match result.map_err(|_| "stdin-read-error")? {
                    Some(json) => {
                        ws_sink
                            .send(Message::Text(json.into()))
                            .await
                            .map_err(|_| "websocket-write-error")?;
                    }
                    None => break, // browser closed the connection
                }
            }
            // WebSocket (Tauri) → stdout (Chrome)
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        write_native_msg(&mut stdout, text.as_str())
                            .await
                            .map_err(|_| "stdout-write-error")?;
                    }
                    None | Some(Err(_)) => break, // server closed
                    _ => {} // binary/ping frames — ignore
                }
            }
        }
    }

    Ok(())
}

fn read_ipc_port() -> Result<u16, &'static str> {
    let appdata = std::env::var("APPDATA").map_err(|_| "desktop-unavailable")?;
    let path = std::path::Path::new(&appdata)
        .join("espass")
        .join("ipc.port");
    let text = std::fs::read_to_string(&path).map_err(|_| "desktop-unavailable")?;
    text.trim().parse::<u16>().map_err(|_| "desktop-unavailable")
}

/// Read one length-prefixed message from `reader`.
/// Returns `Ok(None)` when stdin is closed cleanly.
async fn read_native_msg<R>(reader: &mut R) -> std::io::Result<Option<String>>
where
    R: AsyncReadExt + Unpin,
{
    let mut len_bytes = [0u8; 4];
    match reader.read_exact(&mut len_bytes).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_bytes) as usize;
    if len > 1024 * 1024 {
        return Err(std::io::Error::other("message too large"));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    String::from_utf8(buf)
        .map(Some)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Write one length-prefixed message to `writer`.
async fn write_native_msg<W>(writer: &mut W, text: &str) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let bytes = text.as_bytes();
    if bytes.len() > 1024 * 1024 {
        return Err(std::io::Error::other("message too large"));
    }
    writer.write_all(&(bytes.len() as u32).to_le_bytes()).await?;
    writer.write_all(bytes).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn native_msg_round_trip() {
        let text = r#"{"type":"ping","request_id":"abc-123"}"#;
        let mut buf: Vec<u8> = Vec::new();
        write_native_msg(&mut buf, text).await.unwrap();
        let mut cursor = std::io::Cursor::new(buf);
        let decoded = read_native_msg(&mut cursor).await.unwrap();
        assert_eq!(decoded, Some(text.to_string()));
    }

    #[tokio::test]
    async fn empty_stdin_returns_none() {
        let mut cursor = std::io::Cursor::new(vec![]);
        let result = read_native_msg(&mut cursor).await.unwrap();
        assert_eq!(result, None);
    }
}
```

- [ ] **Step 5: Run tests — expect PASS**

```
cargo test -p espass-native-messaging-host
```

Expected: `native_msg_round_trip ... ok`, `empty_stdin_returns_none ... ok`

- [ ] **Step 6: Release build check**

```
cargo build -p espass-native-messaging-host --release
```

Expected: produces `target/release/espass-host.exe` (Windows) without errors.

- [ ] **Step 7: Commit**

```
git add apps/desktop/native-messaging-host/src/main.rs apps/desktop/native-messaging-host/Cargo.toml
git commit -m "feat(host): replace IPC session protocol with simple stdio-WebSocket bridge"
```

---

### Task 6: Extension manifest + build setup

**Files:**
- Modify: `apps/extension/manifest.chrome.json`
- Create: `apps/extension/manifest.json`
- Modify: `package.json`

- [ ] **Step 1: Update `apps/extension/manifest.chrome.json`**

Add the `action` field for the popup (used when loading the extension from source in dev):

```json
{
  "manifest_version": 3,
  "name": "ESPASS",
  "version": "0.1.0",
  "description": "Zero-knowledge enterprise password manager extension.",
  "permissions": ["activeTab", "scripting", "nativeMessaging"],
  "optional_host_permissions": ["https://*/*"],
  "background": {
    "service_worker": "src/background/service-worker.js",
    "type": "module"
  },
  "content_scripts": [
    {
      "matches": ["https://*/*"],
      "js": ["src/content/autofill-guard.js"],
      "run_at": "document_idle"
    }
  ],
  "action": {
    "default_popup": "src/popup/popup.html",
    "default_title": "ESPASS"
  },
  "content_security_policy": {
    "extension_pages": "default-src 'self'; script-src 'self'; style-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"
  },
  "host_permissions": []
}
```

- [ ] **Step 2: Create `apps/extension/manifest.json`**

This is the manifest for the bundled/dist extension (used for loading from `apps/extension/` with JS files in `dist/`):

```json
{
  "manifest_version": 3,
  "name": "ESPASS",
  "version": "0.1.0",
  "description": "Zero-knowledge enterprise password manager extension.",
  "permissions": ["activeTab", "scripting", "nativeMessaging"],
  "background": {
    "service_worker": "dist/background.js",
    "type": "module"
  },
  "content_scripts": [
    {
      "matches": ["https://*/*"],
      "js": ["dist/content.js"],
      "run_at": "document_idle"
    }
  ],
  "action": {
    "default_popup": "dist/popup.html",
    "default_title": "ESPASS"
  },
  "content_security_policy": {
    "extension_pages": "default-src 'self'; script-src 'self'; style-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"
  }
}
```

- [ ] **Step 3: Update `package.json`**

```json
{
  "name": "espass",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "security-lab": "node security-lab/run-security-lab.mjs",
    "extension:build": "node -e \"require('fs').mkdirSync('apps/extension/dist',{recursive:true})\" && esbuild apps/extension/src/background/service-worker.ts --bundle --outfile=apps/extension/dist/background.js --format=esm --target=chrome120 && esbuild apps/extension/src/content/autofill-guard.ts --bundle --outfile=apps/extension/dist/content.js --format=esm --target=chrome120 && esbuild apps/extension/src/popup/popup.ts --bundle --outfile=apps/extension/dist/popup.js --format=esm --target=chrome120 && node -e \"require('fs').copyFileSync('apps/extension/src/popup/popup.html','apps/extension/dist/popup.html')\""
  },
  "devDependencies": {
    "esbuild": "^0.21.0"
  }
}
```

- [ ] **Step 4: Install esbuild**

```
npm install
```

Expected: `node_modules/esbuild` installed.

- [ ] **Step 5: Commit**

```
git add apps/extension/manifest.chrome.json apps/extension/manifest.json package.json package-lock.json
git commit -m "feat(extension): add popup action to manifest, add esbuild build script"
```

---

### Task 7: Extension service worker rewrite

**Files:**
- Replace: `apps/extension/src/background/service-worker.ts`

- [ ] **Step 1: Write the new `service-worker.ts`**

The existing file connects a new native host port on every credential request. The new version maintains one persistent port, routes responses by `request_id`, and tracks content script ports for push notifications.

```typescript
/**
 * ESPASS background service worker (MV3).
 *
 * - One persistent native-messaging port shared across all tabs.
 * - Response routing by request_id (random UUID added to every outgoing message).
 * - Credential cache: Map<origin, Credential[]> — cleared on vault_locked.
 * - Content script long-lived ports: used to push vault_locked events.
 * - Popup messages: handled via chrome.runtime.onMessage (sendMessage).
 */

const HOST_NAME = "com.espass.desktop";

interface Credential {
  id: string;
  title: string;
  username: string;
}

interface PendingRequest {
  resolve: (value: Record<string, unknown>) => void;
  timeoutId: ReturnType<typeof setTimeout>;
}

// ---------------------------------------------------------------------------
// Native host connection
// ---------------------------------------------------------------------------

let nativePort: chrome.runtime.Port | null = null;
const pendingRequests = new Map<string, PendingRequest>();
const credentialCache = new Map<string, Credential[]>(); // origin → items

function getOrConnectNativeHost(): chrome.runtime.Port {
  if (nativePort) return nativePort;

  nativePort = chrome.runtime.connectNative(HOST_NAME);
  nativePort.onMessage.addListener(handleNativeMessage);
  nativePort.onDisconnect.addListener(() => {
    nativePort = null;
    for (const [id, pending] of pendingRequests) {
      clearTimeout(pending.timeoutId);
      pending.resolve({ type: "error", code: "native-host-disconnected" });
      pendingRequests.delete(id);
    }
    broadcastToContentPorts({ type: "vault_status", state: "unavailable" });
  });

  return nativePort;
}

function sendToNativeHost(
  msg: Record<string, unknown>
): Promise<Record<string, unknown>> {
  return new Promise((resolve) => {
    const requestId = crypto.randomUUID();
    msg.request_id = requestId;

    const timeoutId = setTimeout(() => {
      pendingRequests.delete(requestId);
      resolve({ type: "error", code: "timeout" });
    }, 10_000);

    pendingRequests.set(requestId, { resolve, timeoutId });
    getOrConnectNativeHost().postMessage(msg);
  });
}

function handleNativeMessage(msg: unknown): void {
  if (!msg || typeof msg !== "object") return;
  const m = msg as Record<string, unknown>;

  if (m.type === "vault_locked") {
    credentialCache.clear();
    broadcastToContentPorts({ type: "vault_locked" });
    return;
  }

  const requestId = m.request_id as string | undefined;
  if (!requestId) return;

  const pending = pendingRequests.get(requestId);
  if (!pending) return;

  clearTimeout(pending.timeoutId);
  pendingRequests.delete(requestId);
  pending.resolve(m);
}

// ---------------------------------------------------------------------------
// Content script long-lived ports
// ---------------------------------------------------------------------------

const contentPorts: chrome.runtime.Port[] = [];

function broadcastToContentPorts(msg: unknown): void {
  for (let i = contentPorts.length - 1; i >= 0; i--) {
    try {
      contentPorts[i].postMessage(msg);
    } catch {
      contentPorts.splice(i, 1);
    }
  }
}

chrome.runtime.onConnect.addListener((port) => {
  if (port.name !== "espass-content") return;

  contentPorts.push(port);
  port.onDisconnect.addListener(() => {
    const idx = contentPorts.indexOf(port);
    if (idx !== -1) contentPorts.splice(idx, 1);
  });

  port.onMessage.addListener(
    async (msg: Record<string, unknown>) => {
      if (!msg || typeof msg.type !== "string") return;
      const requestId = msg.request_id as string | undefined;
      let response: Record<string, unknown>;

      switch (msg.type) {
        case "find_credentials": {
          const origin = msg.origin as string;
          const cached = credentialCache.get(origin);
          if (cached) {
            response = { type: "credentials", items: cached };
          } else {
            response = await sendToNativeHost({ type: "find_credentials", origin });
            if (response.type === "credentials") {
              credentialCache.set(origin, response.items as Credential[]);
            }
          }
          break;
        }
        case "fill_credential": {
          const raw = await sendToNativeHost({
            type: "get_credential",
            id: msg.id as string,
          });
          if (raw.type === "credential") {
            response = {
              type: "fill_data",
              username: raw.username,
              password: raw.password,
            };
          } else {
            response = raw;
          }
          break;
        }
        case "get_vault_status": {
          response = await resolveVaultStatus();
          break;
        }
        default:
          return;
      }

      if (requestId) {
        port.postMessage({ ...response, request_id: requestId });
      }
    }
  );
});

// ---------------------------------------------------------------------------
// Popup messages (sendMessage — not long-lived ports)
// ---------------------------------------------------------------------------

chrome.runtime.onMessage.addListener(
  (
    message: Record<string, unknown>,
    _sender: chrome.runtime.MessageSender,
    sendResponse: (response: unknown) => void
  ) => {
    if (!message || typeof message.type !== "string") return false;

    switch (message.type) {
      case "get_vault_status": {
        resolveVaultStatus().then(sendResponse);
        return true;
      }
      case "unlock": {
        sendToNativeHost({
          type: "unlock",
          password: message.password as string,
        }).then(sendResponse);
        return true;
      }
      case "lock": {
        sendToNativeHost({ type: "lock" }).then((raw) => {
          credentialCache.clear();
          sendResponse(raw);
        });
        return true;
      }
      default:
        return false;
    }
  }
);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function resolveVaultStatus(): Promise<Record<string, unknown>> {
  try {
    const raw = await sendToNativeHost({ type: "status" });
    if (raw.type === "status") {
      const state = raw.vault_state === "unlocked" ? "ready" : "locked";
      return {
        type: "vault_status",
        state,
        autolock_minutes: raw.autolock_minutes ?? null,
      };
    }
    if (raw.code === "native-host-disconnected") {
      return { type: "vault_status", state: "unavailable" };
    }
    return { type: "vault_status", state: "locked" };
  } catch {
    return { type: "vault_status", state: "unavailable" };
  }
}
```

- [ ] **Step 2: Build check**

```
npm run extension:build
```

Expected: `apps/extension/dist/background.js` created, no TypeScript errors.

- [ ] **Step 3: Commit**

```
git add apps/extension/src/background/service-worker.ts
git commit -m "feat(extension): rewrite service worker — persistent port, response routing, credential cache"
```

---

### Task 8: Content script rewrite + `dropdown.ts`

**Files:**
- Replace: `apps/extension/src/content/autofill-guard.ts`
- Create: `apps/extension/src/content/dropdown.ts`

- [ ] **Step 1: Create `apps/extension/src/content/dropdown.ts`**

```typescript
/**
 * ESPASS autofill dropdown — Shadow DOM component.
 *
 * Isolated from page CSS and scripts. Keyboard: ↓/↑ navigate,
 * Enter selects, Escape/Tab dismiss. Click outside dismisses.
 */

export interface CredentialItem {
  id: string;
  title: string;
  username: string;
}

let currentHost: HTMLElement | null = null;
let cleanupFns: Array<() => void> = [];

export function dismissDropdown(): void {
  for (const fn of cleanupFns) fn();
  cleanupFns = [];
  currentHost?.remove();
  currentHost = null;
}

export function showDropdown(
  anchor: HTMLInputElement,
  items: CredentialItem[],
  onSelect: (id: string) => void
): void {
  dismissDropdown(); // remove any previous dropdown

  const host = document.createElement("div");
  host.setAttribute("data-espass-dropdown", "");
  const shadow = host.attachShadow({ mode: "closed" });

  const style = document.createElement("style");
  style.textContent = `
    .dropdown {
      position: fixed;
      z-index: 2147483647;
      background: #fff;
      border: 1px solid #d0d5dd;
      border-radius: 8px;
      box-shadow: 0 8px 24px rgba(0,0,0,.12);
      min-width: 220px;
      max-width: 380px;
      overflow: hidden;
      font-family: system-ui, -apple-system, sans-serif;
      font-size: 14px;
    }
    .item {
      display: flex;
      flex-direction: column;
      padding: 8px 14px;
      cursor: pointer;
      outline: none;
      user-select: none;
    }
    .item:hover, .item.active {
      background: #f0f4ff;
    }
    .item-title  { font-weight: 600; color: #101828; }
    .item-user   { font-size: 12px; color: #667085; margin-top: 1px; }
  `;
  shadow.appendChild(style);

  const dropdown = document.createElement("div");
  dropdown.className = "dropdown";
  shadow.appendChild(dropdown);

  const els: HTMLDivElement[] = [];
  let activeIdx = -1;

  function setActive(idx: number): void {
    els[activeIdx]?.classList.remove("active");
    activeIdx = idx;
    els[activeIdx]?.classList.add("active");
  }

  items.forEach((item, i) => {
    const el = document.createElement("div");
    el.className = "item";
    el.tabIndex = 0;
    el.setAttribute("role", "option");
    el.innerHTML =
      `<span class="item-title">🔑 ${esc(item.title)}</span>` +
      `<span class="item-user">${esc(item.username)}</span>`;
    el.addEventListener("click", () => { dismissDropdown(); onSelect(item.id); });
    el.addEventListener("mouseenter", () => setActive(i));
    dropdown.appendChild(el);
    els.push(el);
  });

  // Position below the anchor field
  const rect = anchor.getBoundingClientRect();
  Object.assign(dropdown.style, {
    top: `${rect.bottom + 4}px`,
    left: `${rect.left}px`,
  });

  // Keyboard handler on the document
  const onKey = (e: KeyboardEvent): void => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive(Math.min(activeIdx + 1, els.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive(Math.max(activeIdx - 1, 0));
    } else if (e.key === "Enter" && activeIdx >= 0) {
      e.preventDefault();
      dismissDropdown();
      onSelect(items[activeIdx].id);
    } else if (e.key === "Escape" || e.key === "Tab") {
      dismissDropdown();
    }
  };

  // Click-outside handler
  const onClickOutside = (e: MouseEvent): void => {
    if (e.target !== anchor && !host.contains(e.target as Node)) {
      dismissDropdown();
    }
  };

  document.addEventListener("keydown", onKey);
  document.addEventListener("click", onClickOutside, { capture: true });
  cleanupFns.push(() => {
    document.removeEventListener("keydown", onKey);
    document.removeEventListener("click", onClickOutside, { capture: true });
  });

  document.body.appendChild(host);
  currentHost = host;
}

function esc(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
```

- [ ] **Step 2: Rewrite `apps/extension/src/content/autofill-guard.ts`**

```typescript
/**
 * ESPASS content script — autofill guard.
 *
 * Listens for clicks on password fields only, requests matching credentials
 * from the background service worker, and shows the Shadow DOM dropdown.
 * Fills both username and password fields after the user selects a credential.
 *
 * Security guards (overlay detection, cross-origin iframe, suspicious domain)
 * are preserved from the original implementation.
 */

import { detectFullscreenOverlay, checkOverlay } from "./overlay-guard";
import { showDropdown, dismissDropdown, type CredentialItem } from "./dropdown";

// ---------------------------------------------------------------------------
// Signal helpers (unchanged from original)
// ---------------------------------------------------------------------------

export function isVisibleInput(element: HTMLInputElement): boolean {
  const style = window.getComputedStyle(element);
  const rect  = element.getBoundingClientRect();
  return (
    style.visibility !== "hidden" &&
    style.display    !== "none"   &&
    Number(style.opacity) > 0     &&
    rect.width  >= 8              &&
    rect.height >= 8              &&
    !element.disabled             &&
    element.type !== "hidden"
  );
}

export function detectSuspiciousDomain(hostname: string): boolean {
  const ascii = hostname.toLowerCase();
  return (
    ascii.startsWith("xn--") ||
    ascii.includes(".xn--")  ||
    /[^\x00-\x7F]/u.test(hostname)
  );
}

// ---------------------------------------------------------------------------
// Password-field detection
// ---------------------------------------------------------------------------

function isPasswordField(input: HTMLInputElement): boolean {
  if (input.type === "password") return true;
  const ac = (input.getAttribute("autocomplete") ?? "").toLowerCase();
  return ac.includes("current-password") || ac.includes("new-password");
}

// ---------------------------------------------------------------------------
// Background communication (long-lived port)
// ---------------------------------------------------------------------------

let bgPort: chrome.runtime.Port | null = null;
const pending = new Map<string, (r: Record<string, unknown>) => void>();

function getBgPort(): chrome.runtime.Port {
  if (bgPort) return bgPort;

  bgPort = chrome.runtime.connect({ name: "espass-content" });

  bgPort.onMessage.addListener((msg: Record<string, unknown>) => {
    if (msg.type === "vault_locked") {
      dismissDropdown();
      return;
    }
    const rid = msg.request_id as string | undefined;
    if (!rid) return;
    const resolve = pending.get(rid);
    if (resolve) {
      pending.delete(rid);
      resolve(msg);
    }
  });

  bgPort.onDisconnect.addListener(() => {
    bgPort = null;
    dismissDropdown();
    // Reject any pending requests
    for (const [id, resolve] of pending) {
      resolve({ type: "error", code: "disconnected" });
      pending.delete(id);
    }
  });

  return bgPort;
}

function sendToBg(
  msg: Record<string, unknown>
): Promise<Record<string, unknown>> {
  return new Promise((resolve) => {
    const requestId = crypto.randomUUID();
    msg.request_id = requestId;
    pending.set(requestId, resolve);
    getBgPort().postMessage(msg);
  });
}

// ---------------------------------------------------------------------------
// Click listener — entry point
// ---------------------------------------------------------------------------

document.addEventListener(
  "click",
  async (event) => {
    const target = event.target;
    if (!(target instanceof HTMLInputElement)) return;
    if (!isPasswordField(target)) return;

    // Security guards
    if (detectFullscreenOverlay()) return;
    const overlayResult = checkOverlay(event.clientX, event.clientY, target);
    if (!overlayResult.safe) return;

    const origin = window.location.origin;
    let topLevelOrigin = origin;
    try { topLevelOrigin = window.top?.location.origin ?? origin; }
    catch { topLevelOrigin = "cross-origin"; }

    if (topLevelOrigin !== origin) return; // cross-origin iframe
    if (detectSuspiciousDomain(window.location.hostname)) return;
    if (!isVisibleInput(target)) return;

    const response = await sendToBg({ type: "find_credentials", origin });
    if (response.type !== "credentials") return;

    const items = response.items as CredentialItem[];
    if (items.length === 0) return;

    showDropdown(target, items, async (id) => {
      const fillResponse = await sendToBg({ type: "fill_credential", id });
      if (fillResponse.type === "fill_data") {
        fillFields(
          target,
          fillResponse.username as string,
          fillResponse.password as string
        );
      }
    });
  },
  { capture: true }
);

// ---------------------------------------------------------------------------
// Field filling
// ---------------------------------------------------------------------------

function fillFields(
  passwordInput: HTMLInputElement,
  username: string,
  password: string
): void {
  const form = passwordInput.closest("form") ?? document.body;
  const candidates = Array.from(
    form.querySelectorAll<HTMLInputElement>(
      'input[type="text"], input[type="email"], input:not([type])'
    )
  );

  // First visible input that appears before the password field in DOM order
  const usernameInput =
    candidates.find((el) => {
      if (!isVisibleInput(el) || el === passwordInput) return false;
      return (
        el.compareDocumentPosition(passwordInput) &
        Node.DOCUMENT_POSITION_FOLLOWING
      );
    }) ?? null;

  if (usernameInput) setNativeValue(usernameInput, username);
  setNativeValue(passwordInput, password);
}

/** Set input value in a way that React / Vue / Angular detect as a real change. */
function setNativeValue(input: HTMLInputElement, value: string): void {
  const descriptor = Object.getOwnPropertyDescriptor(
    window.HTMLInputElement.prototype,
    "value"
  );
  descriptor?.set?.call(input, value);
  input.dispatchEvent(new Event("input",  { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
}
```

- [ ] **Step 3: Build check**

```
npm run extension:build
```

Expected: `content.js` in `dist/` — no TypeScript errors.

- [ ] **Step 4: Commit**

```
git add apps/extension/src/content/autofill-guard.ts apps/extension/src/content/dropdown.ts
git commit -m "feat(extension): password-field autofill with Shadow DOM dropdown"
```

---

### Task 9: Extension popup (`popup.html` + `popup.ts`)

**Files:**
- Create: `apps/extension/src/popup/popup.html`
- Create: `apps/extension/src/popup/popup.ts`

- [ ] **Step 1: Create `apps/extension/src/popup/popup.html`**

```html
<!DOCTYPE html>
<html lang="da">
<head>
  <meta charset="UTF-8">
  <title>ESPASS</title>
  <style>
    * { box-sizing: border-box; }
    body {
      width: 280px; margin: 0; padding: 16px;
      font-family: system-ui, -apple-system, sans-serif;
      font-size: 14px; color: #101828; background: #fff;
    }
    h1 { font-size: 16px; margin: 0 0 4px; }
    p  { margin: 0 0 12px; color: #667085; font-size: 13px; }
    input[type="password"] {
      width: 100%; padding: 8px 10px;
      border: 1px solid #d0d5dd; border-radius: 6px;
      font-size: 14px; margin-bottom: 8px; outline: none;
    }
    input[type="password"]:focus { border-color: #4f6ef7; }
    button {
      width: 100%; padding: 9px;
      border: none; border-radius: 6px;
      background: #4f6ef7; color: #fff;
      font-size: 14px; font-weight: 600; cursor: pointer;
    }
    button:hover    { background: #3a57d4; }
    button:disabled { background: #a0aec0; cursor: not-allowed; }
    .muted  { font-size: 12px; color: #98a2b3; margin-top: 6px; }
    #error  { color: #c0392b; font-size: 12px; margin-top: 4px; min-height: 16px; }
  </style>
</head>
<body>
  <div id="root"></div>
  <script type="module" src="popup.js"></script>
</body>
</html>
```

- [ ] **Step 2: Create `apps/extension/src/popup/popup.ts`**

```typescript
/**
 * ESPASS extension popup.
 *
 * Three states rendered in the same #root shell:
 *   unavailable — desktop app not running
 *   locked      — vault locked, shows master-password form
 *   ready       — vault unlocked, shows status + lock button
 */

const root = document.getElementById("root")!;

type VaultState = "ready" | "locked" | "unavailable";

// ---------------------------------------------------------------------------
// State query
// ---------------------------------------------------------------------------

interface VaultStatusResponse {
  type: string;
  state: VaultState;
  autolock_minutes?: number | null;
}

async function getVaultStatus(): Promise<VaultStatusResponse> {
  return new Promise((resolve) => {
    chrome.runtime.sendMessage(
      { type: "get_vault_status" },
      (response: VaultStatusResponse | undefined) => {
        if (chrome.runtime.lastError || !response) {
          resolve({ type: "vault_status", state: "unavailable" });
          return;
        }
        resolve(response);
      }
    );
  });
}

// ---------------------------------------------------------------------------
// Renderers
// ---------------------------------------------------------------------------

function renderUnavailable(): void {
  root.innerHTML = `
    <h1>⚠️ ESPASS kører ikke</h1>
    <p>Start ESPASS-appen for at bruge autofill</p>
    <button id="open-btn">Åbn ESPASS</button>
  `;
  document.getElementById("open-btn")!.addEventListener("click", () => {
    // Attempt deep-link; falls back silently if OS doesn't handle espass://
    chrome.tabs.create({ url: "espass://" });
    window.close();
  });
}

function renderLocked(): void {
  root.innerHTML = `
    <h1>🔒 ESPASS er låst</h1>
    <p>Skriv dit master password for at låse op</p>
    <input type="password" id="master-pw" placeholder="Master password" autocomplete="current-password">
    <button id="unlock-btn">Lås op</button>
    <div id="error"></div>
  `;

  const pwInput   = document.getElementById("master-pw")   as HTMLInputElement;
  const unlockBtn = document.getElementById("unlock-btn")  as HTMLButtonElement;
  const errorEl   = document.getElementById("error")!;

  pwInput.focus();

  async function doUnlock(): Promise<void> {
    const password = pwInput.value.trim();
    if (!password) return;

    unlockBtn.disabled = true;
    unlockBtn.textContent = "Låser op…";
    errorEl.textContent = "";

    const response = await new Promise<Record<string, unknown>>((resolve) => {
      chrome.runtime.sendMessage({ type: "unlock", password }, resolve);
    });

    if (response?.ok === true) {
      renderReady(null); // autolock shown as "–" until status refetched
      getVaultStatus().then((s) => renderReady(s.autolock_minutes ?? null));
    } else {
      unlockBtn.disabled = false;
      unlockBtn.textContent = "Lås op";
      errorEl.textContent = "Forkert password – prøv igen.";
    }
  }

  unlockBtn.addEventListener("click", doUnlock);
  pwInput.addEventListener("keydown", (e) => { if (e.key === "Enter") doUnlock(); });
}

function renderReady(autolockMinutes: number | null): void {
  const autolockText =
    autolockMinutes != null ? `${autolockMinutes} min` : "Aldrig";

  root.innerHTML = `
    <h1>✅ ESPASS klar</h1>
    <p class="muted">Auto-lock: ${autolockText}</p>
    <button id="lock-btn">Lås vault</button>
  `;

  document.getElementById("lock-btn")!.addEventListener("click", () => {
    chrome.runtime.sendMessage({ type: "lock" }, () => {
      renderLocked();
    });
  });
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

(async () => {
  root.innerHTML = `<p class="muted">Henter status…</p>`;
  const status = await getVaultStatus();

  switch (status.state) {
    case "unavailable": renderUnavailable(); break;
    case "locked":      renderLocked();      break;
    case "ready":       renderReady(status.autolock_minutes ?? null); break;
    default:            renderUnavailable();
  }
})();
```

- [ ] **Step 3: Build check**

```
npm run extension:build
```

Expected: `dist/background.js`, `dist/content.js`, `dist/popup.js`, `dist/popup.html` all present.

- [ ] **Step 4: Full Rust build check**

```
cargo build -p espass-desktop -p espass-native-messaging-host
```

Expected: both crates compile without errors.

- [ ] **Step 5: Run all Rust tests**

```
cargo test -p espass-desktop -p espass-native-messaging-host
```

Expected: all tests pass, including:
- `ipc_state_tests::touch_vault_access_updates_timestamp`
- `tests::etld1_https_extracts_last_two_labels`
- `tests::etld1_rejects_http`
- `tests::etld1_rejects_single_label`
- `tests::credential_matches_same_etld1`
- `tests::credential_rejects_different_domain`
- `lock_cmd_tests::lock_status_default_values`
- `tray::tests::should_autolock_when_threshold_exceeded`
- `tray::tests::should_not_autolock_when_recently_accessed`
- `native_messaging_host::tests::native_msg_round_trip`
- `native_messaging_host::tests::empty_stdin_returns_none`

- [ ] **Step 6: Commit**

```
git add apps/extension/src/popup/popup.html apps/extension/src/popup/popup.ts
git commit -m "feat(extension): add popup with unavailable/locked/ready states"
```

---

## Self-Review

### 1. Spec coverage

| Spec section | Covered by |
|---|---|
| System tray with lock/unlock icon | Task 3 `tray.rs` |
| Autostart via tauri-plugin-autostart | Task 3 + Task 4 lib.rs |
| Auto-lock timer (default 15 min, configurable) | Task 3 timer + Task 4 `set_autolock_timeout` |
| WebSocket IPC server on `127.0.0.1:0`, writes `ipc.port` | Task 2 `ipc_server.rs` |
| IPC: `find_credentials`, `get_credential`, `unlock`, `lock`, `status` | Task 2 |
| IPC: `vault_locked` push on lock | Task 2 broadcast |
| eTLD+1 matching, HTTPS only | Task 2 |
| Passwords excluded from `find_credentials` response | Task 2 (only id/title/username) |
| `espass-host` simple stdio↔WebSocket bridge | Task 5 |
| Native host reads `ipc.port`, exits with error if missing | Task 5 |
| Chrome MV3 manifest with nativeMessaging + action | Task 6 |
| Service worker: persistent native port, response routing | Task 7 |
| Service worker: credential cache per origin, cleared on lock | Task 7 |
| Content script: only trigger on password fields | Task 8 |
| Shadow DOM dropdown: keyboard nav, dismiss on click-outside | Task 8 dropdown.ts |
| Field fill with React/Vue/Angular event dispatch | Task 8 `setNativeValue` |
| Popup: three states (unavailable / locked / ready) | Task 9 |
| Popup: unlock form sends master password | Task 9 |
| Popup: "Lås vault" button | Task 9 |
| esbuild `extension:build` script | Task 6 |

### 2. Placeholder scan

No TBD, TODO, or placeholder steps found. All steps contain complete code.

### 3. Type consistency

- `Credential` (commands.rs pub(crate)) used in ipc_server.rs ✓
- `CredentialItem` interface defined in dropdown.ts and imported in autofill-guard.ts ✓
- `VaultState` type consistent across service-worker.ts and popup.ts ✓
- `lock_notify_tx: broadcast::Sender<()>` created in `AppState::new`, subscribed in `handle_connection` ✓
