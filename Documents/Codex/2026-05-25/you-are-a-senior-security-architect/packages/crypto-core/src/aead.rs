use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use serde::{Deserialize, Serialize};

use crate::{random_array, CryptoError, MasterKey, SecureBuffer, SessionKey, VaultKey};

const NONCE_LEN: usize = 12;
const ENVELOPE_VERSION: u8 = 1;
const MAX_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;
const STREAM_CHUNK_BYTES: usize = 1024 * 1024;

/// Authenticated metadata bound to encrypted payloads as associated data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptionMetadata {
    /// Tenant or organization identifier.
    pub tenant_id: String,
    /// Vault identifier.
    pub vault_id: String,
    /// Optional item identifier.
    pub item_id: Option<String>,
    /// Payload record type.
    pub record_type: String,
    /// Schema version bound into authentication.
    pub schema_version: u16,
}

impl EncryptionMetadata {
    /// Deterministic associated-data encoding.
    #[must_use]
    pub fn to_aad(&self) -> Vec<u8> {
        format!(
            "espass:aad:v1\ntenant={}\nvault={}\nitem={}\ntype={}\nschema={}",
            self.tenant_id,
            self.vault_id,
            self.item_id.as_deref().unwrap_or(""),
            self.record_type,
            self.schema_version
        )
        .into_bytes()
    }
}

/// Versioned authenticated-encryption envelope for encrypted ESPASS payloads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedEnvelope {
    /// Envelope version. Version 1 uses AES-256-GCM and a 96-bit nonce.
    pub version: u8,
    /// Random AES-GCM nonce.
    pub nonce: [u8; NONCE_LEN],
    /// Authenticated ciphertext and tag.
    pub ciphertext: Vec<u8>,
}

/// Encrypts plaintext with AES-256-GCM and contextual associated data.
pub fn encrypt(
    key: &VaultKey,
    plaintext: &[u8],
    associated_data: &[u8],
) -> Result<EncryptedEnvelope, CryptoError> {
    encrypt_with_key_bytes(key.expose_secret(), plaintext, associated_data)
}

/// Encrypts a key envelope with a password-derived master key.
pub fn encrypt_with_master_key(
    key: &MasterKey,
    plaintext: &[u8],
    associated_data: &[u8],
) -> Result<EncryptedEnvelope, CryptoError> {
    encrypt_with_key_bytes(key.expose_secret(), plaintext, associated_data)
}

/// Decrypts a key envelope with a password-derived master key.
pub fn decrypt_with_master_key(
    key: &MasterKey,
    envelope: &EncryptedEnvelope,
    associated_data: &[u8],
) -> Result<SecureBuffer, CryptoError> {
    decrypt_with_key_bytes(key.expose_secret(), envelope, associated_data)
}

fn encrypt_with_key_bytes(
    key: &[u8; 32],
    plaintext: &[u8],
    associated_data: &[u8],
) -> Result<EncryptedEnvelope, CryptoError> {
    if plaintext.len() > MAX_PAYLOAD_BYTES {
        return Err(CryptoError::PayloadTooLarge);
    }

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::InvalidKey)?;
    let nonce = random_array()?;

    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: associated_data,
            },
        )
        .map_err(|_| CryptoError::EncryptionFailed)?;

    Ok(EncryptedEnvelope {
        version: ENVELOPE_VERSION,
        nonce,
        ciphertext,
    })
}

/// Decrypts an ESPASS encrypted envelope with AES-256-GCM.
pub fn decrypt(
    key: &VaultKey,
    envelope: &EncryptedEnvelope,
    associated_data: &[u8],
) -> Result<SecureBuffer, CryptoError> {
    decrypt_with_key_bytes(key.expose_secret(), envelope, associated_data)
}

fn decrypt_with_key_bytes(
    key: &[u8; 32],
    envelope: &EncryptedEnvelope,
    associated_data: &[u8],
) -> Result<SecureBuffer, CryptoError> {
    if envelope.version != ENVELOPE_VERSION {
        return Err(CryptoError::UnsupportedEnvelopeVersion);
    }
    if envelope.ciphertext.len() > MAX_PAYLOAD_BYTES + 16 {
        return Err(CryptoError::PayloadTooLarge);
    }

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::InvalidKey)?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&envelope.nonce),
            Payload {
                msg: envelope.ciphertext.as_ref(),
                aad: associated_data,
            },
        )
        .map_err(|_| CryptoError::DecryptionFailed)?;

    Ok(SecureBuffer::new(plaintext))
}

/// Streaming-style chunk encryptor.
///
/// This MVP splits input into independently authenticated chunks. Each chunk
/// binds the caller AAD plus a monotonically increasing chunk index and final
/// marker, so chunk reordering and truncation are detected by the caller when
/// all chunks are verified in sequence.
pub struct StreamingEncryptor<'a> {
    key: &'a VaultKey,
    base_aad: Vec<u8>,
    index: u64,
}

impl<'a> StreamingEncryptor<'a> {
    /// Creates a streaming encryptor for large local payloads.
    #[must_use]
    pub fn new(key: &'a VaultKey, associated_data: &[u8]) -> Self {
        Self {
            key,
            base_aad: associated_data.to_vec(),
            index: 0,
        }
    }

    /// Encrypts a single chunk.
    pub fn encrypt_chunk(
        &mut self,
        plaintext: &[u8],
        is_final: bool,
    ) -> Result<EncryptedEnvelope, CryptoError> {
        if plaintext.len() > STREAM_CHUNK_BYTES {
            return Err(CryptoError::PayloadTooLarge);
        }
        let aad = chunk_aad(&self.base_aad, self.index, is_final);
        let envelope = encrypt(self.key, plaintext, &aad)?;
        self.index = self.index.saturating_add(1);
        Ok(envelope)
    }
}

/// Streaming-style chunk decryptor.
pub struct StreamingDecryptor<'a> {
    key: &'a VaultKey,
    base_aad: Vec<u8>,
    index: u64,
}

impl<'a> StreamingDecryptor<'a> {
    /// Creates a streaming decryptor for large local payloads.
    #[must_use]
    pub fn new(key: &'a VaultKey, associated_data: &[u8]) -> Self {
        Self {
            key,
            base_aad: associated_data.to_vec(),
            index: 0,
        }
    }

    /// Decrypts a single chunk in order.
    pub fn decrypt_chunk(
        &mut self,
        envelope: &EncryptedEnvelope,
        is_final: bool,
    ) -> Result<SecureBuffer, CryptoError> {
        let aad = chunk_aad(&self.base_aad, self.index, is_final);
        let plaintext = decrypt(self.key, envelope, &aad)?;
        self.index = self.index.saturating_add(1);
        Ok(plaintext)
    }
}

fn chunk_aad(base: &[u8], index: u64, is_final: bool) -> Vec<u8> {
    let mut aad = Vec::with_capacity(base.len() + 32);
    aad.extend_from_slice(base);
    aad.extend_from_slice(b"\nchunk=");
    aad.extend_from_slice(index.to_string().as_bytes());
    aad.extend_from_slice(b"\nfinal=");
    aad.extend_from_slice(if is_final { b"1" } else { b"0" });
    aad
}

/// Encrypts a session payload using a session key mapped into the vault-key
/// AEAD implementation. Session keys are separate types to prevent accidental
/// key mixing at call sites.
pub fn encrypt_session_payload(
    key: &SessionKey,
    plaintext: &[u8],
    associated_data: &[u8],
) -> Result<EncryptedEnvelope, CryptoError> {
    encrypt_with_key_bytes(key.expose_secret(), plaintext, associated_data)
}

/// Returns the current ESPASS encrypted envelope version.
#[must_use]
pub fn envelope_version() -> u8 {
    ENVELOPE_VERSION
}
