//! Zero-knowledge cloud sync: HKDF auth derivation, push, pull.

#[allow(unused_imports)]
use espass_crypto_core::{MasterKey, VaultKey, EncryptedEnvelope};
#[allow(unused_imports)]
use espass_shared_types::vault::{EncryptedPayload, VaultItem};
use hkdf::Hkdf;
use sha2::Sha256;
use time::OffsetDateTime;
use uuid::Uuid;

#[allow(unused_imports)]
use crate::commands::{Credential, VaultContents};
use crate::state::{AppState, ItemSyncRecord, SyncState, SyncStateFile, SyncStatus};

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("vault is locked")]
    VaultLocked,
    #[error("sync not configured")]
    NotConfigured,
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("server returned {0}")]
    ServerError(u16),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid uuid: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("mutex poisoned")]
    Poison,
}

impl From<reqwest::Error> for SyncError {
    fn from(e: reqwest::Error) -> Self {
        SyncError::Http(e.to_string())
    }
}

// ── HKDF auth-secret derivation ───────────────────────────────────────────────

/// Derives a 32-byte auth_secret from the master_key using HKDF-SHA256.
/// This is deterministic and re-derivable at each unlock — never stored.
pub fn derive_auth_secret(master_key: &MasterKey) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, master_key.expose_secret());
    let mut out = [0u8; 32];
    hk.expand(b"espass:auth:v1", &mut out)
        .expect("HKDF-SHA256: output len <= 255 * hash_len");
    out
}

// ── sync_state.json helpers ───────────────────────────────────────────────────

pub fn load_sync_state_file(state: &AppState) -> Result<SyncStateFile, SyncError> {
    let bytes = std::fs::read(state.sync_state_path())?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn save_sync_state_file(state: &AppState, sf: &SyncStateFile) -> Result<(), SyncError> {
    std::fs::create_dir_all(&state.vault_dir)?;
    let json = serde_json::to_string_pretty(sf)?;
    std::fs::write(state.sync_state_path(), json.as_bytes())?;
    Ok(())
}

// ── Configure (register or login) ─────────────────────────────────────────────

#[derive(serde::Serialize)]
struct RegisterRequest<'a> {
    email: &'a str,
    auth_secret_hex: String,
}

#[derive(serde::Deserialize)]
struct RegisterResponse {
    user_id: String,
    vault_id: String,
}

#[derive(serde::Serialize)]
struct LoginRequest<'a> {
    email: &'a str,
    auth_secret_hex: String,
}

#[derive(serde::Deserialize)]
struct LoginResponse {
    jwt: String,
    refresh_token: String,
    user_id: String,
    vault_id: String,
}

/// Registers or logs in, then stores JWT in RAM and SyncStateFile on disk.
/// `auth_secret` must be derived by the caller via `derive_auth_secret`.
pub async fn configure(
    server_url: &str,
    email: &str,
    auth_secret: &[u8; 32],
    register: bool,
    state: &AppState,
) -> Result<(), SyncError> {
    let client = reqwest::Client::new();
    let hex_secret = hex::encode(auth_secret);
    let base = server_url.trim_end_matches('/');

    if register {
        let res = client
            .post(format!("{base}/v1/auth/register"))
            .json(&RegisterRequest { email, auth_secret_hex: hex_secret.clone() })
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(SyncError::ServerError(res.status().as_u16()));
        }
    }

    let res = client
        .post(format!("{base}/v1/auth/login"))
        .json(&LoginRequest { email, auth_secret_hex: hex_secret })
        .send()
        .await?;

    if !res.status().is_success() {
        return Err(SyncError::ServerError(res.status().as_u16()));
    }

    let login: LoginResponse = res.json().await?;
    let jwt_expires_at = jwt_exp_from_token(&login.jwt);
    let vault_id = Uuid::parse_str(&login.vault_id)?;
    let user_id = Uuid::parse_str(&login.user_id)?;

    // Persist config (no secrets)
    let sf = SyncStateFile {
        server_url: server_url.to_string(),
        user_id: login.user_id.clone(),
        vault_id: login.vault_id.clone(),
        last_synced_at: None,
        pending_deletes: Vec::new(),
        items: std::collections::HashMap::new(),
    };
    save_sync_state_file(state, &sf)?;

    // Store JWT in RAM
    let mut sync = state.sync.lock().map_err(|_| SyncError::Poison)?;
    *sync = Some(SyncState {
        server_url: server_url.to_string(),
        user_id,
        vault_id,
        jwt: login.jwt,
        refresh_token: login.refresh_token,
        jwt_expires_at,
        last_synced_at: None,
        status: SyncStatus::Idle { last_synced_at: 0 },
    });
    Ok(())
}

/// Extracts the `exp` Unix timestamp from a JWT without full verification.
/// Falls back to now + 900 seconds if parsing fails.
pub fn jwt_exp_from_token(token: &str) -> i64 {
    let fallback = OffsetDateTime::now_utc().unix_timestamp() + 900;
    (|| -> Option<i64> {
        let payload = token.split('.').nth(1)?;
        use base64::Engine;
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .ok()?;
        let v: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
        v.get("exp")?.as_i64()
    })()
    .unwrap_or(fallback)
}

pub fn get_status(state: &AppState) -> SyncStatus {
    state.sync
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|s| s.status.clone()))
        .unwrap_or(SyncStatus::NotConfigured)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_secret_is_deterministic() {
        let key = MasterKey::from_bytes([0xAB; 32]);
        let s1 = derive_auth_secret(&key);
        let s2 = derive_auth_secret(&key);
        assert_eq!(s1, s2);
    }

    #[test]
    fn auth_secret_differs_from_master_key() {
        let key = MasterKey::from_bytes([0xAB; 32]);
        let secret = derive_auth_secret(&key);
        assert_ne!(&secret, key.expose_secret());
    }

    #[test]
    fn different_master_keys_give_different_secrets() {
        let k1 = MasterKey::from_bytes([0x11; 32]);
        let k2 = MasterKey::from_bytes([0x22; 32]);
        assert_ne!(derive_auth_secret(&k1), derive_auth_secret(&k2));
    }
}
