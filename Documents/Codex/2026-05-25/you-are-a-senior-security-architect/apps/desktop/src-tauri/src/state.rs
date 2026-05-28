//! Shared Tauri application state.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, Mutex};

use espass_crypto_core::{EncryptedEnvelope, KdfParams, Salt};
use espass_vault_runtime::{RuntimeSecretStore, SessionRuntime, UnlockManager};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Vault metadata stored on disk (unencrypted — contains encrypted vault key only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultMeta {
    pub vault_id: Uuid,
    pub kdf_params: KdfParams,
    pub salt: Salt,
    pub encrypted_vault_key: EncryptedEnvelope,
    /// Monotonic revision of the last saved vault data file.
    pub data_revision: u64,
}

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
    /// Configured auto-lock duration in minutes. `None` = never.
    /// Default is `Some(15)` — locks after 15 minutes of inactivity.
    pub autolock_minutes: Mutex<Option<u32>>,
    /// Unix timestamp of the last vault access (find/get/unlock). Used by auto-lock timer.
    pub last_vault_access: Arc<AtomicI64>,
    /// Port the IPC WebSocket server is listening on (set after startup).
    pub ipc_port: Mutex<Option<u16>>,
    /// Random 64-char hex token written to ipc.port alongside the port number.
    /// Clients must send it as the first WebSocket message to authenticate.
    pub ipc_token: Mutex<Option<String>>,
}

impl Default for AppState {
    /// Resolves the vault directory from OS data-dir conventions.
    /// Windows: `%APPDATA%\espass`
    /// Linux/macOS: `$XDG_DATA_HOME/espass` → `$HOME/.local/share/espass` → `.espass`
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
        // Load persisted auto-lock setting; fall back to 15-minute default.
        let autolock_minutes = Self::load_persisted_autolock(&vault_dir).unwrap_or(Some(15));
        Self {
            secrets: Mutex::new(RuntimeSecretStore::locked()),
            session: Mutex::new(None),
            unlock_manager: Mutex::new(UnlockManager::new(KdfParams::default())),
            vault_dir,
            sync: Mutex::new(None),
            lock_notify_tx,
            autolock_minutes: Mutex::new(autolock_minutes),
            last_vault_access: Arc::new(AtomicI64::new(0)),
            ipc_port: Mutex::new(None),
            ipc_token: Mutex::new(None),
        }
    }

    /// Path to the vault metadata file (KDF params, salt, encrypted vault key).
    #[must_use]
    pub fn meta_path(&self) -> PathBuf {
        self.vault_dir.join("vault.meta.json")
    }

    /// Path to the encrypted vault data file (credentials blob).
    #[must_use]
    pub fn data_path(&self) -> PathBuf {
        self.vault_dir.join("vault.data.json")
    }

    /// Returns true when the vault has been set up.
    #[must_use]
    pub fn vault_exists(&self) -> bool {
        self.meta_path().exists()
    }

    /// Path to the sync state file (no secrets — not encrypted).
    #[must_use]
    pub fn sync_state_path(&self) -> PathBuf {
        self.vault_dir.join("sync_state.json")
    }

    /// Records that the vault was accessed right now; resets the auto-lock timer.
    /// The store uses `Release` ordering so the auto-lock timer can use `Acquire`
    /// on the load and see a consistent view of all prior vault state writes.
    pub fn touch_vault_access(&self) {
        use std::sync::atomic::Ordering;
        self.last_vault_access.store(
            time::OffsetDateTime::now_utc().unix_timestamp(),
            Ordering::Release,
        );
    }

    /// Path to the persisted settings file (auto-lock timeout, etc.).
    #[must_use]
    pub fn settings_path(&self) -> PathBuf {
        self.vault_dir.join("settings.json")
    }

    /// Set the auto-lock timeout in memory **and** persist it to `settings.json`.
    /// `None` means never auto-lock.
    pub fn set_autolock_minutes(&self, minutes: Option<u32>) {
        if let Ok(mut m) = self.autolock_minutes.lock() {
            *m = minutes;
        }
        let _ = std::fs::create_dir_all(&self.vault_dir);
        let json = serde_json::json!({ "autolock_minutes": minutes });
        let _ = std::fs::write(self.settings_path(), json.to_string());
    }

    /// Load persisted autolock setting from `settings.json`.
    /// Returns `Some(None)` for "never lock", `Some(Some(n))` for n minutes,
    /// and `None` when the file is missing or the key is absent (use default).
    fn load_persisted_autolock(vault_dir: &PathBuf) -> Option<Option<u32>> {
        let path = vault_dir.join("settings.json");
        let text = std::fs::read_to_string(path).ok()?;
        let v: serde_json::Value = serde_json::from_str(&text).ok()?;
        match v.get("autolock_minutes")? {
            serde_json::Value::Null      => Some(None),
            serde_json::Value::Number(n) => Some(n.as_u64().map(|m| m as u32)),
            _ => None,
        }
    }
}

// ── Sync state ────────────────────────────────────────────────────────────────

/// Per-item sync record stored in sync_state.json (no secrets).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ItemSyncRecord {
    pub server_revision: u64,
    pub last_pushed_at: i64,
}

/// Sync configuration and item tracking file — written to disk, no secrets.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

/// Live sync state held in RAM — contains JWT and refresh token.
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

/// Observable sync status — safe to serialize and send to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SyncStatus {
    NotConfigured,
    Idle { last_synced_at: i64 },
    Syncing,
    Error { message: String },
    Unauthenticated,
}

#[cfg(test)]
mod sync_state_tests {
    use super::*;

    #[test]
    fn sync_state_file_round_trips() {
        let sf = SyncStateFile {
            server_url: "https://sync.example.com".into(),
            user_id: "user-1".into(),
            vault_id: "vault-1".into(),
            last_synced_at: Some(1_000_000),
            pending_deletes: vec!["id-a".into()],
            items: {
                let mut m = HashMap::new();
                m.insert("id-a".into(), ItemSyncRecord { server_revision: 3, last_pushed_at: 999_000 });
                m
            },
        };
        let json = serde_json::to_string(&sf).unwrap();
        let back: SyncStateFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.server_url, sf.server_url);
        assert_eq!(back.pending_deletes, sf.pending_deletes);
        assert_eq!(back.items["id-a"].server_revision, 3);
    }

    #[test]
    fn sync_status_not_configured_serializes() {
        let s = SyncStatus::NotConfigured;
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("not_configured"));
    }
}

#[cfg(test)]
mod ipc_state_tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn ipc_token_defaults_to_none() {
        let state = AppState::new(PathBuf::from("test-token-state"));
        let token = state.ipc_token.lock().unwrap();
        assert!(token.is_none(), "ipc_token should start as None");
    }

    #[test]
    fn touch_vault_access_updates_timestamp() {
        let state = AppState::new(std::path::PathBuf::from("test-vault-ipc"));
        let before = state.last_vault_access.load(Ordering::Relaxed);
        assert_eq!(before, 0);
        state.touch_vault_access();
        let after = state.last_vault_access.load(Ordering::Relaxed);
        assert!(after > 0, "timestamp should be set after touch");
    }

    #[test]
    fn lock_notify_tx_broadcasts_to_subscriber() {
        let state = AppState::new(std::path::PathBuf::from("test-vault-notify"));
        let mut rx = state.lock_notify_tx.subscribe();
        state.lock_notify_tx.send(()).expect("send failed");
        rx.try_recv().expect("subscriber should have received the notification");
    }
}
