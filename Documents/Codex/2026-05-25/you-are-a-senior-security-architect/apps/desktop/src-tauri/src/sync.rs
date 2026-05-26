//! Zero-knowledge cloud sync: HKDF auth derivation, push, pull.

use espass_crypto_core::{MasterKey, VaultKey, EncryptedEnvelope};
use espass_shared_types::vault::{EncryptedPayload, VaultItem};
use hkdf::Hkdf;
use sha2::Sha256;
use time::OffsetDateTime;
use uuid::Uuid;

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

// ── Per-item encryption / decryption ─────────────────────────────────────────

pub fn encrypt_credential(
    cred: &Credential,
    vault_key: &VaultKey,
    vault_id: &str,
) -> Result<VaultItem, SyncError> {
    let plaintext = serde_json::to_vec(cred)?;
    let aad = format!("espass:item:v1:{vault_id}:{}", cred.id);
    let envelope =
        espass_crypto_core::encrypt(vault_key, &plaintext, aad.as_bytes())
            .map_err(|e| SyncError::Crypto(e.to_string()))?;

    let item_id = Uuid::parse_str(&cred.id)?;
    let vault_uuid = Uuid::parse_str(vault_id)?;

    Ok(VaultItem {
        item_id,
        vault_id: vault_uuid,
        encrypted_payload: EncryptedPayload {
            envelope_version: envelope.version,
            nonce: envelope.nonce,
            ciphertext: envelope.ciphertext,
            aad_context: aad,
        },
        attachments: vec![],
        revision: 0,
        base_revision: None,
        created_at: OffsetDateTime::from_unix_timestamp(cred.created_at)
            .unwrap_or_else(|_| OffsetDateTime::now_utc()),
        updated_at: OffsetDateTime::from_unix_timestamp(cred.updated_at)
            .unwrap_or_else(|_| OffsetDateTime::now_utc()),
        deleted_at: None,
    })
}

pub fn decrypt_item(item: &VaultItem, vault_key: &VaultKey) -> Result<Credential, SyncError> {
    let p = &item.encrypted_payload;
    let envelope = EncryptedEnvelope {
        version: p.envelope_version,
        nonce: p.nonce,
        ciphertext: p.ciphertext.clone(),
    };
    let plaintext = espass_crypto_core::decrypt(vault_key, &envelope, p.aad_context.as_bytes())
        .map_err(|e| SyncError::Crypto(e.to_string()))?;
    let cred: Credential = serde_json::from_slice(plaintext.expose_secret())?;
    Ok(cred)
}

// ── Push (local → server) ─────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct SyncItemMetaResponse {
    item_id: String,
    updated_at: i64,
    revision: u64,
    deleted_at: Option<i64>,
}

/// Pushes dirty credentials and pending deletes to the server.
/// Returns (pushed_count, conflicts_skipped).
pub async fn push_all(
    client: &reqwest::Client,
    jwt: &str,
    base_url: &str,
    vault_id: &str,
    contents: &VaultContents,
    sf: &mut SyncStateFile,
    vault_key: &VaultKey,
) -> Result<(u32, u32), SyncError> {
    let mut pushed = 0u32;
    let mut conflicts_skipped = 0u32;

    for cred in &contents.credentials {
        let last_pushed = sf
            .items
            .get(&cred.id)
            .map(|r| r.last_pushed_at)
            .unwrap_or(0);

        if cred.updated_at <= last_pushed {
            continue; // not dirty
        }

        let base_rev = sf.items.get(&cred.id).map(|r| r.server_revision);
        let mut item = encrypt_credential(cred, vault_key, vault_id)?;
        item.base_revision = base_rev;

        let url = format!("{base_url}/v1/vaults/{vault_id}/items/{}", cred.id);
        let res = client
            .put(&url)
            .bearer_auth(jwt)
            .json(&item)
            .send()
            .await?;

        match res.status().as_u16() {
            204 => {
                sf.items.insert(
                    cred.id.clone(),
                    ItemSyncRecord {
                        server_revision: base_rev.unwrap_or(0) + 1,
                        last_pushed_at: cred.updated_at,
                    },
                );
                pushed += 1;
            }
            409 => {
                conflicts_skipped += 1;
            }
            code => return Err(SyncError::ServerError(code)),
        }
    }

    // Push pending deletes
    let deletes = std::mem::take(&mut sf.pending_deletes);
    for id in deletes {
        let base_rev = sf.items.get(&id).map(|r| r.server_revision);
        let now = OffsetDateTime::now_utc();
        let tombstone = Credential {
            id: id.clone(), title: String::new(), username: String::new(),
            password: String::new(), url: None,
            created_at: now.unix_timestamp(), updated_at: now.unix_timestamp(),
        };
        let mut item = encrypt_credential(&tombstone, vault_key, vault_id)?;
        item.deleted_at = Some(now);
        item.base_revision = base_rev;

        let url = format!("{base_url}/v1/vaults/{vault_id}/items/{id}");
        let res = client.put(&url).bearer_auth(jwt).json(&item).send().await?;
        if res.status().as_u16() != 204 && res.status().as_u16() != 409 {
            // Re-queue if failed
            sf.pending_deletes.push(id);
        }
    }

    Ok((pushed, conflicts_skipped))
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

    #[test]
    fn encrypt_decrypt_credential_round_trip() {
        let vault_key = VaultKey::from_bytes([0x55; 32]);
        let vault_id = "aaaaaaaa-0000-0000-0000-000000000001";
        let cred = Credential {
            id: "bbbbbbbb-0000-0000-0000-000000000002".into(),
            title: "Test".into(),
            username: "user@test.com".into(),
            password: "p@ss".into(),
            url: Some("https://test.com".into()),
            created_at: 1_000_000,
            updated_at: 1_000_001,
        };
        let item = encrypt_credential(&cred, &vault_key, vault_id).unwrap();
        let back = decrypt_item(&item, &vault_key).unwrap();
        assert_eq!(back.title, cred.title);
        assert_eq!(back.password, cred.password);
    }

    #[test]
    fn wrong_key_fails_decrypt() {
        let vault_key = VaultKey::from_bytes([0x55; 32]);
        let wrong_key = VaultKey::from_bytes([0x66; 32]);
        let vault_id = "aaaaaaaa-0000-0000-0000-000000000001";
        let cred = Credential {
            id: "bbbbbbbb-0000-0000-0000-000000000002".into(),
            title: "T".into(), username: "u".into(), password: "p".into(),
            url: None, created_at: 0, updated_at: 0,
        };
        let item = encrypt_credential(&cred, &vault_key, vault_id).unwrap();
        assert!(decrypt_item(&item, &wrong_key).is_err());
    }
}
