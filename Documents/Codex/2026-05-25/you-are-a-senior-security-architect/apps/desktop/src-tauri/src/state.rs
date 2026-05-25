//! Shared Tauri application state.

use std::sync::Mutex;

use espass_vault_runtime::RuntimeSecretStore;

/// Tauri-managed application state.
pub struct AppState {
    /// Encrypted runtime secret store protected by a mutex.
    pub secrets: Mutex<RuntimeSecretStore>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            secrets: Mutex::new(RuntimeSecretStore::locked()),
        }
    }
}
