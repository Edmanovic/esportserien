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

// ── Pull (server → local) ─────────────────────────────────────────────────────

/// Pulls new/updated/deleted items from the server and merges into contents.
/// Returns (pulled_count, deleted_count).
pub async fn pull_all(
    client: &reqwest::Client,
    jwt: &str,
    base_url: &str,
    vault_id: &str,
    contents: &mut VaultContents,
    sf: &mut SyncStateFile,
    vault_key: &VaultKey,
) -> Result<(u32, u32), SyncError> {
    let mut pulled = 0u32;
    let mut deleted = 0u32;

    // 1. Get metadata list from server
    let url = format!("{base_url}/v1/vaults/{vault_id}/items");
    let res = client.get(&url).bearer_auth(jwt).send().await?;
    if !res.status().is_success() {
        return Err(SyncError::ServerError(res.status().as_u16()));
    }
    let metas: Vec<SyncItemMetaResponse> = res.json().await?;

    for meta in metas {
        // Handle deletes from server
        if meta.deleted_at.is_some() {
            let before = contents.credentials.len();
            contents.credentials.retain(|c| c.id != meta.item_id);
            if contents.credentials.len() < before {
                deleted += 1;
            }
            sf.items.remove(&meta.item_id);
            continue;
        }

        // Check if server version is newer than local
        let local_ts = contents
            .credentials
            .iter()
            .find(|c| c.id == meta.item_id)
            .map(|c| c.updated_at)
            .unwrap_or(0);

        if meta.updated_at <= local_ts {
            continue; // local is newer or equal
        }

        // Fetch full item
        let item_url = format!("{base_url}/v1/vaults/{vault_id}/items/{}", meta.item_id);
        let item_res = client.get(&item_url).bearer_auth(jwt).send().await?;
        if !item_res.status().is_success() {
            continue; // skip this item, try next time
        }
        let item: VaultItem = item_res.json().await?;

        match decrypt_item(&item, vault_key) {
            Ok(cred) => {
                // Merge: replace existing or push new
                if let Some(existing) = contents.credentials.iter_mut().find(|c| c.id == cred.id) {
                    *existing = cred;
                } else {
                    contents.credentials.push(cred);
                }
                sf.items.insert(
                    meta.item_id.clone(),
                    ItemSyncRecord {
                        server_revision: meta.revision,
                        last_pushed_at: meta.updated_at,
                    },
                );
                pulled += 1;
            }
            Err(_) => {} // skip corrupted items silently
        }
    }

    Ok((pulled, deleted))
}

// ── sync_now ──────────────────────────────────────────────────────────────────

pub struct SyncResult {
    pub pushed: u32,
    pub pulled: u32,
    pub deleted: u32,
    pub conflicts_skipped: u32,
}

/// Full sync cycle: refresh JWT if needed → push → pull → save state.
pub async fn sync_now(state: &AppState) -> Result<SyncResult, SyncError> {
    use espass_vault_runtime::{VaultPersistenceEngine, VaultStore};

    // 1. Extract RAM state (release lock before async work)
    let (server_url, vault_id_str, mut jwt, refresh_token, jwt_expires_at) = {
        let sync = state.sync.lock().map_err(|_| SyncError::Poison)?;
        let s = sync.as_ref().ok_or(SyncError::NotConfigured)?;
        (
            s.server_url.clone(),
            s.vault_id.to_string(),
            s.jwt.clone(),
            s.refresh_token.clone(),
            s.jwt_expires_at,
        )
    };

    // 2. Refresh JWT if expires in < 60 seconds
    let now = OffsetDateTime::now_utc().unix_timestamp();
    if jwt_expires_at - now < 60 {
        let client = reqwest::Client::new();
        let res = client
            .post(format!("{}/v1/auth/refresh", server_url.trim_end_matches('/')))
            .json(&serde_json::json!({ "refresh_token": refresh_token }))
            .send()
            .await?;
        if res.status().is_success() {
            #[derive(serde::Deserialize)]
            struct RefreshResp { jwt: String, refresh_token: String, user_id: String, vault_id: String }
            let r: RefreshResp = res.json().await?;
            let new_exp = jwt_exp_from_token(&r.jwt);
            let mut sync = state.sync.lock().map_err(|_| SyncError::Poison)?;
            if let Some(s) = sync.as_mut() {
                s.jwt = r.jwt.clone();
                s.refresh_token = r.refresh_token;
                s.jwt_expires_at = new_exp;
            }
            jwt = r.jwt;
        }
    }

    // 3. Extract vault key bytes (drop lock before async work)
    let key_bytes: [u8; 32] = {
        let secrets = state.secrets.lock().map_err(|_| SyncError::Poison)?;
        let key = secrets.vault_key().map_err(|_| SyncError::VaultLocked)?;
        *key.expose_secret()
    };
    let vault_key = VaultKey::from_bytes(key_bytes);

    // 4. Load vault meta + contents
    let mut vault_meta: crate::state::VaultMeta = {
        let bytes = std::fs::read(state.meta_path())?;
        serde_json::from_slice(&bytes)?
    };

    let engine = VaultPersistenceEngine::new(VaultStore::new(state.data_path()));
    let mut contents: VaultContents = if state.data_path().exists() {
        let buf = engine.load_decrypted(&vault_key)
            .map_err(|e| SyncError::Crypto(e.to_string()))?;
        serde_json::from_slice(buf.expose_secret())?
    } else {
        VaultContents::default()
    };

    let mut sf = load_sync_state_file(state).unwrap_or_else(|_| SyncStateFile {
        server_url: server_url.clone(),
        user_id: String::new(),
        vault_id: vault_id_str.clone(),
        last_synced_at: None,
        pending_deletes: Vec::new(),
        items: std::collections::HashMap::new(),
    });

    // 5. Set status to Syncing
    {
        let mut sync = state.sync.lock().map_err(|_| SyncError::Poison)?;
        if let Some(s) = sync.as_mut() {
            s.status = crate::state::SyncStatus::Syncing;
        }
    }

    let client = reqwest::Client::new();
    let base = server_url.trim_end_matches('/');

    // 6. Push
    let (pushed, conflicts_skipped) =
        push_all(&client, &jwt, base, &vault_id_str, &contents, &mut sf, &vault_key).await?;

    // 7. Pull
    let (pulled, deleted) =
        pull_all(&client, &jwt, base, &vault_id_str, &mut contents, &mut sf, &vault_key).await?;

    // 8. Save merged vault contents
    let json = serde_json::to_vec(&contents)?;
    let record = engine
        .persist(&vault_key, vault_meta.vault_id, &json, Some(vault_meta.data_revision))
        .map_err(|e| SyncError::Crypto(e.to_string()))?;
    vault_meta.data_revision = record.local_revision;
    let meta_json = serde_json::to_vec_pretty(&vault_meta)?;
    std::fs::write(state.meta_path(), &meta_json)?;

    // 9. Save sync state file and update RAM status
    let now_ts = OffsetDateTime::now_utc().unix_timestamp();
    sf.last_synced_at = Some(now_ts);
    save_sync_state_file(state, &sf)?;

    {
        let mut sync = state.sync.lock().map_err(|_| SyncError::Poison)?;
        if let Some(s) = sync.as_mut() {
            s.last_synced_at = Some(now_ts);
            s.status = crate::state::SyncStatus::Idle { last_synced_at: now_ts };
        }
    }

    Ok(SyncResult { pushed, pulled, deleted, conflicts_skipped })
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
