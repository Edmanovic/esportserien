//! Shared Tauri application state.

use std::path::PathBuf;
use std::sync::Mutex;

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
        Self {
            secrets: Mutex::new(RuntimeSecretStore::locked()),
            session: Mutex::new(None),
            unlock_manager: Mutex::new(UnlockManager::new(KdfParams::default())),
            vault_dir,
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
}
