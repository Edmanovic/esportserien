//! Encrypted local vault persistence.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use espass_crypto_core::{
    decrypt, encrypt, EncryptedEnvelope, EncryptionMetadata, SecureBuffer, VaultKey,
};
use espass_shared_types::vault::VAULT_SCHEMA_VERSION;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::RuntimeError;

type HmacSha256 = Hmac<Sha256>;

const LOCAL_STORE_FORMAT_VERSION: u16 = 1;

/// Server-safe encrypted local vault record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalVaultRecord {
    /// Local store schema version.
    pub store_version: u16,
    /// Vault schema version.
    pub vault_schema_version: u16,
    /// Vault UUID.
    pub vault_id: Uuid,
    /// Monotonic local revision used for rollback detection.
    pub local_revision: u64,
    /// Highest synchronized revision seen from backend.
    pub sync_revision: u64,
    /// Encrypted vault contents.
    pub encrypted_payload: EncryptedEnvelope,
    /// Record creation timestamp.
    pub created_at: OffsetDateTime,
    /// Record update timestamp.
    pub updated_at: OffsetDateTime,
    /// HMAC over deterministic record fields using the vault key.
    pub integrity_tag: [u8; 32],
}

/// Filesystem-backed local vault store.
#[derive(Debug, Clone)]
pub struct VaultStore {
    path: PathBuf,
}

impl VaultStore {
    /// Creates a local vault store at the given path.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Returns true when the vault file exists.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Reads and validates an encrypted vault record.
    pub fn load(&self, key: &VaultKey) -> Result<LocalVaultRecord, RuntimeError> {
        let mut file = File::open(&self.path).map_err(|_| RuntimeError::Persistence)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|_| RuntimeError::Persistence)?;
        let record: LocalVaultRecord =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::Integrity)?;
        VaultIntegrityVerifier::verify(key, &record)?;
        Ok(record)
    }

    /// Atomically writes an encrypted vault record using a same-directory temp file.
    pub fn save(&self, record: &LocalVaultRecord) -> Result<(), RuntimeError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|_| RuntimeError::Persistence)?;
        }
        let temp_path = temp_path_for(&self.path);
        let encoded = serde_json::to_vec(record).map_err(|_| RuntimeError::Persistence)?;

        {
            let mut temp = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
                .map_err(|_| RuntimeError::Persistence)?;
            temp.write_all(&encoded)
                .map_err(|_| RuntimeError::Persistence)?;
            temp.sync_all().map_err(|_| RuntimeError::Persistence)?;
        }

        fs::rename(&temp_path, &self.path).map_err(|_| {
            let _ = fs::remove_file(&temp_path);
            RuntimeError::Persistence
        })?;

        if let Some(parent) = self.path.parent() {
            if let Ok(directory) = File::open(parent) {
                let _ = directory.sync_all();
            }
        }

        Ok(())
    }
}

/// Encrypts and persists vault bytes.
#[derive(Debug, Clone)]
pub struct VaultPersistenceEngine {
    store: VaultStore,
}

impl VaultPersistenceEngine {
    /// Creates a persistence engine.
    #[must_use]
    pub fn new(store: VaultStore) -> Self {
        Self { store }
    }

    /// Encrypts and saves local vault bytes.
    pub fn persist(
        &self,
        key: &VaultKey,
        vault_id: Uuid,
        plaintext: &[u8],
        previous_revision: Option<u64>,
    ) -> Result<LocalVaultRecord, RuntimeError> {
        let now = OffsetDateTime::now_utc();
        let local_revision = previous_revision.unwrap_or(0).saturating_add(1);
        let metadata = EncryptionMetadata {
            tenant_id: "local".to_owned(),
            vault_id: vault_id.to_string(),
            item_id: None,
            record_type: "local-vault".to_owned(),
            schema_version: VAULT_SCHEMA_VERSION,
        };
        let encrypted_payload = encrypt(key, plaintext, &metadata.to_aad())?;
        let mut record = LocalVaultRecord {
            store_version: LOCAL_STORE_FORMAT_VERSION,
            vault_schema_version: VAULT_SCHEMA_VERSION,
            vault_id,
            local_revision,
            sync_revision: 0,
            encrypted_payload,
            created_at: now,
            updated_at: now,
            integrity_tag: [0_u8; 32],
        };
        record.integrity_tag = VaultIntegrityVerifier::tag(key, &record)?;
        self.store.save(&record)?;
        Ok(record)
    }

    /// Loads, validates, and decrypts the local vault.
    pub fn load_decrypted(&self, key: &VaultKey) -> Result<SecureBuffer, RuntimeError> {
        let record = self.store.load(key)?;
        let metadata = EncryptionMetadata {
            tenant_id: "local".to_owned(),
            vault_id: record.vault_id.to_string(),
            item_id: None,
            record_type: "local-vault".to_owned(),
            schema_version: record.vault_schema_version,
        };
        decrypt(key, &record.encrypted_payload, &metadata.to_aad()).map_err(Into::into)
    }
}

/// Vault integrity verifier.
pub struct VaultIntegrityVerifier;

impl VaultIntegrityVerifier {
    /// Computes the local record integrity tag.
    pub fn tag(key: &VaultKey, record: &LocalVaultRecord) -> Result<[u8; 32], RuntimeError> {
        let mut mac =
            HmacSha256::new_from_slice(key.expose_secret()).map_err(|_| RuntimeError::Integrity)?;
        mac.update(b"espass:local-vault:v1\n");
        mac.update(&record.store_version.to_be_bytes());
        mac.update(&record.vault_schema_version.to_be_bytes());
        mac.update(record.vault_id.as_bytes());
        mac.update(&record.local_revision.to_be_bytes());
        mac.update(&record.sync_revision.to_be_bytes());
        mac.update(&[record.encrypted_payload.version]);
        mac.update(&record.encrypted_payload.nonce);
        mac.update(&record.encrypted_payload.ciphertext);
        Ok(mac.finalize().into_bytes().into())
    }

    /// Verifies store format, revision, and integrity tag.
    pub fn verify(key: &VaultKey, record: &LocalVaultRecord) -> Result<(), RuntimeError> {
        if record.store_version != LOCAL_STORE_FORMAT_VERSION {
            return Err(RuntimeError::Migration);
        }
        if record.vault_schema_version != VAULT_SCHEMA_VERSION {
            return Err(RuntimeError::Migration);
        }
        let expected = Self::tag(key, record)?;
        if expected.ct_eq(&record.integrity_tag).into() {
            Ok(())
        } else {
            Err(RuntimeError::Integrity)
        }
    }
}

/// Vault migration engine placeholder for v1+ migrations.
pub struct VaultMigrationEngine;

impl VaultMigrationEngine {
    /// Validates whether a local store can be opened by this runtime.
    pub fn validate_supported(record: &LocalVaultRecord) -> Result<(), RuntimeError> {
        if record.store_version == LOCAL_STORE_FORMAT_VERSION
            && record.vault_schema_version == VAULT_SCHEMA_VERSION
        {
            Ok(())
        } else {
            Err(RuntimeError::Migration)
        }
    }
}

fn temp_path_for(path: &Path) -> PathBuf {
    let mut temp = path.to_path_buf();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map_or_else(|| "tmp".to_owned(), |value| format!("{value}.tmp"));
    temp.set_extension(extension);
    temp
}
