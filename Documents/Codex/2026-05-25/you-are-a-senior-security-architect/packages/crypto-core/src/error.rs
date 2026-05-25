use thiserror::Error;

/// Errors returned by ESPASS cryptographic operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CryptoError {
    /// The provided Argon2id parameter set is invalid.
    #[error("invalid Argon2id parameters")]
    InvalidKdfParameters,
    /// The supplied encryption key has an invalid length.
    #[error("key must be exactly 32 bytes")]
    InvalidKey,
    /// The encrypted envelope version is unsupported.
    #[error("unsupported encrypted envelope version")]
    UnsupportedEnvelopeVersion,
    /// Authenticated encryption failed.
    #[error("encryption failed")]
    EncryptionFailed,
    /// Authenticated decryption failed.
    #[error("decryption failed")]
    DecryptionFailed,
    /// The payload exceeded ESPASS local processing limits.
    #[error("payload exceeds configured limit")]
    PayloadTooLarge,
    /// Secure random generation failed.
    #[error("secure random generation failed")]
    RandomFailed,
    /// Memory locking failed or is unavailable.
    #[error("memory locking failed")]
    MemoryLockFailed,
    /// Input did not match the deterministic serialization profile.
    #[error("invalid serialization")]
    InvalidSerialization,
}
