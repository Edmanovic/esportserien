//! Cryptographic primitives for ESPASS zero-knowledge clients.
//!
//! This crate isolates password key derivation, authenticated encryption,
//! sensitive buffers, memory locking, and key-type boundaries behind small APIs.
//! It intentionally avoids custom cryptographic constructions.

mod aead;
mod buffer;
mod error;
mod kdf;
mod keys;
mod memlock;
mod random;

pub use aead::{
    decrypt, decrypt_with_master_key, encrypt, encrypt_session_payload, encrypt_with_master_key,
    envelope_version, EncryptedEnvelope, EncryptionMetadata, StreamingDecryptor,
    StreamingEncryptor,
};
pub use buffer::SecureBuffer;
pub use error::CryptoError;
pub use kdf::{derive_master_key, derive_master_key_from_buffer, KdfParams, Salt};
pub use keys::{DeviceKey, MasterKey, SessionKey, VaultKey};
pub use memlock::{MemoryLock, MemoryLockGuard};
pub use random::{fill_random, random_array, random_vec};

/// Current ESPASS cryptographic schema version.
pub const CRYPTO_SCHEMA_VERSION: u16 = 1;
