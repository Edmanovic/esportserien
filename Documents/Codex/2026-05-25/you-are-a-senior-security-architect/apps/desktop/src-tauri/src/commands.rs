//! Tauri command handlers with permission-scoped access to vault runtime.

use tauri::State;

use crate::AppState;

/// Response from vault status query.
#[derive(serde::Serialize)]
pub struct SessionStatusResponse {
    /// Whether the vault is currently unlocked.
    pub unlocked: bool,
    /// Whether the vault key's memory pages are OS-pinned (mlock / VirtualLock).
    pub memory_lock_active: bool,
}

/// Unlocks the vault with the given master password.
///
/// In a real implementation this would derive the vault key via Argon2id.
/// For the prototype, this command only wires up the plumbing.
///
/// # Security note
/// The key derivation here is intentionally incomplete — an audit will flag
/// this for Argon2id replacement before any production release.
#[tauri::command]
pub fn unlock_vault(
    password: String,
    vault_id: uuid::Uuid,
    state: State<AppState>,
) -> Result<(), String> {
    #[cfg(not(debug_assertions))]
    compile_error!("unlock_vault uses a placeholder KDF — Argon2id must be wired before release");

    use espass_crypto_core::VaultKey;
    use zeroize::Zeroize;

    let mut secrets = state.secrets.lock().map_err(|e| e.to_string())?;
    // PROTOTYPE: derive key from password bytes truncated/padded to 32 bytes.
    // SECURITY NOTE: This is NOT a secure KDF. Replace with Argon2id before release.
    // The compile_error! above prevents this from shipping in a release build.
    let mut key_bytes = [0u8; 32];
    let password_bytes = password.as_bytes();
    let len = password_bytes.len().min(32);
    key_bytes[..len].copy_from_slice(&password_bytes[..len]);
    let key = VaultKey::from_bytes(key_bytes);
    key_bytes.zeroize();
    // Note: password String heap allocation is dropped here; for production use
    // secrecy::SecretString + explicit zeroize.
    drop(password);
    secrets.open(vault_id, key).map_err(|e| e.to_string())
}

/// Locks the vault and wipes runtime secrets.
#[tauri::command]
pub fn lock_vault(state: State<AppState>) -> Result<(), String> {
    let mut secrets = state.secrets.lock().map_err(|e| e.to_string())?;
    secrets.lock();
    Ok(())
}

/// Returns the current session status without exposing secrets.
#[tauri::command]
pub fn get_session_status(state: State<AppState>) -> Result<SessionStatusResponse, String> {
    let secrets = state.secrets.lock().map_err(|e| e.to_string())?;
    Ok(SessionStatusResponse {
        unlocked: secrets.is_unlocked(),
        memory_lock_active: secrets.memory_lock_active(),
    })
}
