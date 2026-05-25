//! Tauri command handlers with permission-scoped access to vault runtime.

use tauri::State;
use uuid::Uuid;

use crate::state::VaultMeta;
use crate::AppState;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// An item stored inside the encrypted vault.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Credential {
    pub id: String,
    pub title: String,
    pub username: String,
    pub password: String, // plaintext inside encrypted blob
    pub url: Option<String>,
    pub created_at: i64, // Unix timestamp
    pub updated_at: i64,
}

/// Credential list item — no password field.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CredentialSummary {
    pub id: String,
    pub title: String,
    pub username: String,
    pub url: Option<String>,
}

/// The entire vault contents as a JSON blob (encrypted on disk).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct VaultContents {
    pub credentials: Vec<Credential>,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn load_meta(state: &AppState) -> Result<VaultMeta, String> {
    let bytes = std::fs::read(state.meta_path()).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn save_meta(state: &AppState, meta: &VaultMeta) -> Result<(), String> {
    std::fs::create_dir_all(&state.vault_dir).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(meta).map_err(|e| e.to_string())?;
    std::fs::write(state.meta_path(), &bytes).map_err(|e| e.to_string())?;
    Ok(())
}

fn load_contents(
    key: &espass_crypto_core::VaultKey,
    state: &AppState,
) -> Result<VaultContents, String> {
    use espass_vault_runtime::{VaultPersistenceEngine, VaultStore};

    if !state.data_path().exists() {
        return Ok(VaultContents::default());
    }
    let engine = VaultPersistenceEngine::new(VaultStore::new(state.data_path()));
    let buf = engine.load_decrypted(key).map_err(|e| e.to_string())?;
    serde_json::from_slice(buf.expose_secret()).map_err(|e| e.to_string())
}

fn save_contents(
    key: &espass_crypto_core::VaultKey,
    vault_id: Uuid,
    contents: &VaultContents,
    meta: &mut VaultMeta,
    state: &AppState,
) -> Result<(), String> {
    use espass_vault_runtime::{VaultPersistenceEngine, VaultStore};

    let engine = VaultPersistenceEngine::new(VaultStore::new(state.data_path()));
    let json = serde_json::to_vec(contents).map_err(|e| e.to_string())?;
    let record = engine
        .persist(key, vault_id, &json, Some(meta.data_revision))
        .map_err(|e| e.to_string())?;
    meta.data_revision = record.local_revision;
    save_meta(state, meta)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Returns true when a vault has been created on this machine.
#[tauri::command]
pub fn vault_exists(state: State<AppState>) -> bool {
    state.vault_exists()
}

/// Creates a new vault with a master password.
///
/// Runs Argon2id KDF — takes 2-4 seconds.
#[tauri::command]
pub fn create_vault(password: String, state: State<AppState>) -> Result<(), String> {
    use espass_crypto_core::{derive_master_key, encrypt_with_master_key, random_vec, KdfParams, Salt, VaultKey};
    use espass_vault_runtime::{VaultPersistenceEngine, VaultStore};

    if state.vault_exists() {
        return Err("Vault already exists".to_string());
    }

    let vault_id = Uuid::new_v4();
    let kdf_params = KdfParams::default();
    let salt = Salt::generate().map_err(|e| e.to_string())?;
    let master_key =
        derive_master_key(password.as_bytes(), &salt, kdf_params).map_err(|e| e.to_string())?;
    // Best-effort zeroize: String heap bytes are not explicitly zeroed by Rust,
    // but we move/drop immediately to minimize lifetime.
    drop(password);

    // Generate a random 32-byte vault key.
    let vault_key_raw = random_vec(32).map_err(|e| e.to_string())?;
    let aad = format!("espass:vault-key:v1:{vault_id}");
    let encrypted_vault_key =
        encrypt_with_master_key(&master_key, &vault_key_raw, aad.as_bytes())
            .map_err(|e| e.to_string())?;

    // Copy raw bytes into a typed VaultKey.
    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&vault_key_raw);
    let vault_key = VaultKey::from_bytes(key_bytes);

    // Open the secret store before we touch the filesystem.
    state
        .secrets
        .lock()
        .map_err(|e| e.to_string())?
        .open(vault_id, vault_key)
        .map_err(|e| e.to_string())?;

    // Persist an empty vault contents blob.
    // We need to re-borrow the key from the store — hold the lock for the
    // duration of the persist call then release it.
    {
        let secrets = state.secrets.lock().map_err(|e| e.to_string())?;
        let key = secrets.vault_key().map_err(|e| e.to_string())?;
        let contents = VaultContents::default();
        let json = serde_json::to_vec(&contents).map_err(|e| e.to_string())?;
        let engine = VaultPersistenceEngine::new(VaultStore::new(state.data_path()));
        let record = engine
            .persist(key, vault_id, &json, None)
            .map_err(|e| e.to_string())?;

        let meta = VaultMeta {
            vault_id,
            kdf_params,
            salt,
            encrypted_vault_key,
            data_revision: record.local_revision,
        };
        save_meta(&state, &meta)?;
    }

    // Create a session.
    let session =
        espass_vault_runtime::SessionRuntime::new(time::OffsetDateTime::now_utc(), 900, 43_200);
    *state.session.lock().map_err(|e| e.to_string())? = Some(session);

    Ok(())
}

/// Unlocks the vault using the stored Argon2id KDF parameters.
///
/// Takes 2-4 seconds.
#[tauri::command]
pub fn unlock_vault(password: String, state: State<AppState>) -> Result<(), String> {
    use espass_crypto_core::SecureBuffer;

    let meta = load_meta(&state)?;
    let mut pw_buf = SecureBuffer::new(password.into_bytes());

    let now = time::OffsetDateTime::now_utc();

    // Both secrets and unlock_manager must be locked simultaneously to call
    // unlock().  We acquire both here and release after.
    let session = {
        let mut secrets = state.secrets.lock().map_err(|e| e.to_string())?;
        let mut unlock_manager = state.unlock_manager.lock().map_err(|e| e.to_string())?;

        unlock_manager
            .unlock(
                &mut pw_buf,
                &meta.salt,
                &meta.encrypted_vault_key,
                meta.vault_id,
                now,
                &mut secrets,
            )
            .map_err(|_| {
                "Incorrect password or vault locked due to too many attempts".to_string()
            })?
    };

    *state.session.lock().map_err(|e| e.to_string())? = Some(session);
    Ok(())
}

/// Locks the vault and wipes runtime secrets.
#[tauri::command]
pub fn lock_vault(state: State<AppState>) -> Result<(), String> {
    state.secrets.lock().map_err(|e| e.to_string())?.lock();
    *state.session.lock().map_err(|e| e.to_string())? = None;
    Ok(())
}

/// Response from a session status query.
#[derive(serde::Serialize)]
pub struct SessionStatusResponse {
    pub unlocked: bool,
    pub memory_lock_active: bool,
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

/// Lists credentials without exposing passwords.
#[tauri::command]
pub fn list_credentials(state: State<AppState>) -> Result<Vec<CredentialSummary>, String> {
    let secrets = state.secrets.lock().map_err(|e| e.to_string())?;
    let key = secrets.vault_key().map_err(|_| "Vault is locked".to_string())?;
    let contents = load_contents(key, &state)?;
    Ok(contents
        .credentials
        .iter()
        .map(|c| CredentialSummary {
            id: c.id.clone(),
            title: c.title.clone(),
            username: c.username.clone(),
            url: c.url.clone(),
        })
        .collect())
}

/// Adds a new credential and returns its generated UUID.
#[tauri::command]
pub fn add_credential(
    title: String,
    username: String,
    password: String,
    url: Option<String>,
    state: State<AppState>,
) -> Result<String, String> {
    // Extract what we need from the locked store, then release the mutex so
    // save_contents can call save_meta which does I/O.
    let (key_bytes, vault_id) = {
        let secrets = state.secrets.lock().map_err(|e| e.to_string())?;
        let key = secrets.vault_key().map_err(|_| "Vault is locked".to_string())?;
        let mut kb = [0u8; 32];
        kb.copy_from_slice(key.expose_secret());
        let vid = secrets.vault_id().ok_or("Vault is locked")?;
        (kb, vid)
    };
    let vault_key = espass_crypto_core::VaultKey::from_bytes(key_bytes);

    let mut contents = load_contents(&vault_key, &state)?;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let id = Uuid::new_v4().to_string();
    contents.credentials.push(Credential {
        id: id.clone(),
        title,
        username,
        password,
        url,
        created_at: now,
        updated_at: now,
    });
    let mut meta = load_meta(&state)?;
    save_contents(&vault_key, vault_id, &contents, &mut meta, &state)?;
    Ok(id)
}

/// Returns a single credential including its password.
#[tauri::command]
pub fn get_credential(id: String, state: State<AppState>) -> Result<Credential, String> {
    let (key_bytes,) = {
        let secrets = state.secrets.lock().map_err(|e| e.to_string())?;
        let key = secrets.vault_key().map_err(|_| "Vault is locked".to_string())?;
        let mut kb = [0u8; 32];
        kb.copy_from_slice(key.expose_secret());
        (kb,)
    };
    let vault_key = espass_crypto_core::VaultKey::from_bytes(key_bytes);

    let contents = load_contents(&vault_key, &state)?;
    contents
        .credentials
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| "Credential not found".to_string())
}

/// Deletes a credential by UUID.
#[tauri::command]
pub fn delete_credential(id: String, state: State<AppState>) -> Result<(), String> {
    let (key_bytes, vault_id) = {
        let secrets = state.secrets.lock().map_err(|e| e.to_string())?;
        let key = secrets.vault_key().map_err(|_| "Vault is locked".to_string())?;
        let mut kb = [0u8; 32];
        kb.copy_from_slice(key.expose_secret());
        let vid = secrets.vault_id().ok_or("Vault is locked")?;
        (kb, vid)
    };
    let vault_key = espass_crypto_core::VaultKey::from_bytes(key_bytes);

    let mut contents = load_contents(&vault_key, &state)?;
    let before = contents.credentials.len();
    contents.credentials.retain(|c| c.id != id);
    if contents.credentials.len() == before {
        return Err("Credential not found".to_string());
    }
    let mut meta = load_meta(&state)?;
    save_contents(&vault_key, vault_id, &contents, &mut meta, &state)?;
    Ok(())
}

/// Updates an existing credential's fields.
#[tauri::command]
pub fn update_credential(
    id: String,
    title: String,
    username: String,
    password: String,
    url: Option<String>,
    state: State<AppState>,
) -> Result<(), String> {
    let (key_bytes, vault_id) = {
        let secrets = state.secrets.lock().map_err(|e| e.to_string())?;
        let key = secrets.vault_key().map_err(|_| "Vault is locked".to_string())?;
        let mut kb = [0u8; 32];
        kb.copy_from_slice(key.expose_secret());
        let vid = secrets.vault_id().ok_or("Vault is locked")?;
        (kb, vid)
    };
    let vault_key = espass_crypto_core::VaultKey::from_bytes(key_bytes);

    let mut contents = load_contents(&vault_key, &state)?;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let cred = contents
        .credentials
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or_else(|| "Credential not found".to_string())?;
    cred.title = title;
    cred.username = username;
    cred.password = password;
    cred.url = url;
    cred.updated_at = now;

    let mut meta = load_meta(&state)?;
    save_contents(&vault_key, vault_id, &contents, &mut meta, &state)?;
    Ok(())
}

/// Generates a cryptographically secure password from the selected character sets.
/// Uses rejection sampling on OS-CSPRNG bytes to eliminate modulo bias.
/// `length` is clamped to [8, 64].
#[tauri::command]
pub fn generate_password(
    length: u8,
    upper: bool,
    lower: bool,
    digits: bool,
    symbols: bool,
) -> Result<String, String> {
    use espass_crypto_core::random_vec;

    let mut charset = String::new();
    if upper   { charset.push_str("ABCDEFGHIJKLMNOPQRSTUVWXYZ"); }
    if lower   { charset.push_str("abcdefghijklmnopqrstuvwxyz"); }
    if digits  { charset.push_str("0123456789"); }
    if symbols { charset.push_str("!@#$%^&*()-_=+[]{}|;:,.<>?"); }

    if charset.is_empty() {
        return Err("no character set selected".to_string());
    }

    let length = length.clamp(8, 64) as usize;
    let chars: Vec<char> = charset.chars().collect();
    let n = chars.len();
    debug_assert!(n <= 256, "charset larger than byte range — rejection sampling would loop forever");
    let threshold = 256 - (256 % n);

    let mut result = String::with_capacity(length);
    let mut accepted = 0usize;
    while accepted < length {
        let batch = random_vec(length * 2).map_err(|_| "random generation failed".to_string())?;
        for byte in batch {
            if accepted >= length { break; }
            if (byte as usize) < threshold {
                result.push(chars[byte as usize % n]);
                accepted += 1;
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod commands_tests {
    use super::*;

    #[test]
    fn generate_password_correct_length() {
        let pw = generate_password(20, true, true, true, true).unwrap();
        assert_eq!(pw.len(), 20);
    }

    #[test]
    fn generate_password_only_upper() {
        let pw = generate_password(16, true, false, false, false).unwrap();
        assert!(pw.chars().all(|c| c.is_ascii_uppercase()), "unexpected char in: {pw}");
    }

    #[test]
    fn generate_password_clamps_length() {
        let pw = generate_password(4, true, true, false, false).unwrap();
        assert_eq!(pw.len(), 8, "length below 8 should be clamped to 8");

        let pw2 = generate_password(200, true, true, false, false).unwrap();
        assert_eq!(pw2.len(), 64, "length above 64 should be clamped to 64");
    }

    #[test]
    fn generate_password_empty_charset_errors() {
        let err = generate_password(16, false, false, false, false).unwrap_err();
        assert_eq!(err, "no character set selected");
    }
}

// ---------------------------------------------------------------------------
// Import / Export
// ---------------------------------------------------------------------------

/// Summary returned after a CSV import.
#[derive(Debug, serde::Serialize)]
pub struct ImportSummary {
    pub imported: u32,
    pub skipped: u32,
    pub errors: u32,
}

/// Returns (name_col, url_col, user_col, pass_col) indices for a CSV header row.
/// `url_col` is `None` when no URL column exists.
fn detect_csv_columns(
    headers: &csv::StringRecord,
) -> Result<(usize, Option<usize>, usize, usize), String> {
    let find = |candidates: &[&str]| -> Option<usize> {
        candidates.iter().find_map(|c| {
            headers.iter().position(|h| h.eq_ignore_ascii_case(c))
        })
    };

    let name = find(&["name", "title"]).ok_or("Cannot detect name column")?;
    let url  = find(&["url", "login_uri", "formActionOrigin"]);
    let user = find(&["username", "login_username"]).ok_or("Cannot detect username column")?;
    let pass = find(&["password", "login_password"]).ok_or("Cannot detect password column")?;

    Ok((name, url, user, pass))
}

/// Parses a CSV string (Chrome / Firefox / Bitwarden format) and adds
/// credentials to the unlocked vault. Returns an import summary.
#[tauri::command]
pub fn import_credentials(
    csv_text: String,
    state: State<AppState>,
) -> Result<ImportSummary, String> {
    let (key_bytes, vault_id) = {
        let secrets = state.secrets.lock().map_err(|e| e.to_string())?;
        let key = secrets.vault_key().map_err(|_| "Vault is locked".to_string())?;
        let mut kb = [0u8; 32];
        kb.copy_from_slice(key.expose_secret());
        let vid = secrets.vault_id().ok_or("Vault is locked")?;
        (kb, vid)
    };
    let vault_key = espass_crypto_core::VaultKey::from_bytes(key_bytes);

    let mut contents = load_contents(&vault_key, &state)?;
    let mut summary = ImportSummary { imported: 0, skipped: 0, errors: 0 };
    let now = time::OffsetDateTime::now_utc().unix_timestamp();

    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(csv_text.as_bytes());

    let headers = rdr.headers().map_err(|e| e.to_string())?.clone();
    let (col_name, col_url, col_user, col_pass) = detect_csv_columns(&headers)?;

    for result in rdr.records() {
        let record = match result {
            Ok(r) => r,
            Err(_) => { summary.errors += 1; continue; }
        };

        let title    = record.get(col_name).unwrap_or("").trim().to_string();
        let username = record.get(col_user).unwrap_or("").trim().to_string();
        let password = record.get(col_pass).unwrap_or("").trim().to_string();
        let url = col_url
            .and_then(|i| record.get(i))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if username.is_empty() && password.is_empty() {
            summary.skipped += 1;
            continue;
        }

        // Skip duplicates (same title + username, case-insensitive).
        let is_dup = contents.credentials.iter().any(|c| {
            c.title.eq_ignore_ascii_case(&title) && c.username.eq_ignore_ascii_case(&username)
        });
        if is_dup {
            summary.skipped += 1;
            continue;
        }

        contents.credentials.push(Credential {
            id: Uuid::new_v4().to_string(),
            title: if title.is_empty() { username.clone() } else { title },
            username,
            password,
            url,
            created_at: now,
            updated_at: now,
        });
        summary.imported += 1;
    }

    if summary.imported > 0 {
        let mut meta = load_meta(&state)?;
        save_contents(&vault_key, vault_id, &contents, &mut meta, &state)?;
    }

    Ok(summary)
}

/// Exports all credentials as a Chrome-compatible CSV string (plaintext).
#[tauri::command]
pub fn export_credentials_csv(state: State<AppState>) -> Result<String, String> {
    let (key_bytes,) = {
        let secrets = state.secrets.lock().map_err(|e| e.to_string())?;
        let key = secrets.vault_key().map_err(|_| "Vault is locked".to_string())?;
        let mut kb = [0u8; 32];
        kb.copy_from_slice(key.expose_secret());
        (kb,)
    };
    let vault_key = espass_crypto_core::VaultKey::from_bytes(key_bytes);
    let contents = load_contents(&vault_key, &state)?;

    let mut wtr = csv::Writer::from_writer(vec![]);
    wtr.write_record(["name", "url", "username", "password"])
        .map_err(|e| e.to_string())?;
    for c in &contents.credentials {
        wtr.write_record([
            c.title.as_str(),
            c.url.as_deref().unwrap_or(""),
            c.username.as_str(),
            c.password.as_str(),
        ])
        .map_err(|e| e.to_string())?;
    }
    let data = wtr.into_inner().map_err(|e| e.to_string())?;
    String::from_utf8(data).map_err(|e| e.to_string())
}

/// Exports all credentials as a pretty-printed JSON string (plaintext backup).
#[tauri::command]
pub fn export_credentials_json(state: State<AppState>) -> Result<String, String> {
    let (key_bytes,) = {
        let secrets = state.secrets.lock().map_err(|e| e.to_string())?;
        let key = secrets.vault_key().map_err(|_| "Vault is locked".to_string())?;
        let mut kb = [0u8; 32];
        kb.copy_from_slice(key.expose_secret());
        (kb,)
    };
    let vault_key = espass_crypto_core::VaultKey::from_bytes(key_bytes);
    let contents = load_contents(&vault_key, &state)?;
    serde_json::to_string_pretty(&contents).map_err(|e| e.to_string())
}

#[cfg(test)]
mod import_tests {
    use super::*;

    fn make_headers(cols: &[&str]) -> csv::StringRecord {
        csv::StringRecord::from(cols.to_vec())
    }

    #[test]
    fn detect_chrome_format() {
        let h = make_headers(&["name", "url", "username", "password"]);
        let (name, url, user, pass) = detect_csv_columns(&h).unwrap();
        assert_eq!((name, url, user, pass), (0, Some(1), 2, 3));
    }

    #[test]
    fn detect_bitwarden_format() {
        let h = make_headers(&["folder","favorite","type","name","notes","fields",
                               "reprompt","login_uri","login_username","login_password","login_totp"]);
        let (name, url, user, pass) = detect_csv_columns(&h).unwrap();
        assert_eq!(name, 3);
        assert_eq!(url, Some(7));
        assert_eq!(user, 8);
        assert_eq!(pass, 9);
    }

    #[test]
    fn detect_missing_password_errors() {
        let h = make_headers(&["name", "url", "username"]);
        assert!(detect_csv_columns(&h).is_err());
    }

    #[test]
    fn detect_no_url_column() {
        let h = make_headers(&["name", "username", "password"]);
        let (_, url, _, _) = detect_csv_columns(&h).unwrap();
        assert_eq!(url, None);
    }
}
