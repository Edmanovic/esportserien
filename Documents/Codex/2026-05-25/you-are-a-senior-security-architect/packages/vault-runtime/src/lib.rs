//! Secure local ESPASS runtime.
//!
//! This crate owns runtime-only behavior: encrypted local persistence, unlock
//! lifecycle, session expiry, device trust state, IPC sessions, and autofill
//! brokering. It deliberately stores ciphertext on disk and keeps decrypted
//! material only in short-lived in-memory structures.

pub mod autofill;
pub mod clipboard;
pub mod device_trust;
pub mod hardening;
pub mod integrity;
pub mod ipc;
pub mod persistence;
pub mod secret_window;
pub mod sync_client;
pub mod unlock;

pub use autofill::{CredentialRequestBroker, CredentialScope, SecureAutofillRuntime};
pub use clipboard::ClipboardGuard;
pub use device_trust::{DeviceRevocationManager, DeviceTrustRuntime, TrustedDeviceRegistry};
pub use hardening::{catch_vault_panic, sanitize_error, SanitizedError};
pub use integrity::check_runtime_invariants;
pub use ipc::{ExtensionTrustValidator, IpcHandshakeManager, IpcSessionRegistry, SignedIpcRequest};
pub use persistence::{
    LocalVaultRecord, VaultIntegrityVerifier, VaultMigrationEngine, VaultPersistenceEngine,
    VaultStore,
};
pub use secret_window::SecretWindow;
pub use unlock::{AutoLockEngine, RuntimeSecretStore, SessionRuntime, UnlockManager};

/// Runtime errors that must fail closed at trust boundaries.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RuntimeError {
    /// Cryptographic operation failed.
    #[error("cryptographic operation failed")]
    Crypto,
    /// Persistence operation failed.
    #[error("vault persistence failed")]
    Persistence,
    /// Vault integrity validation failed.
    #[error("vault integrity validation failed")]
    Integrity,
    /// Vault schema migration failed or is unsupported.
    #[error("vault migration failed")]
    Migration,
    /// Unlock failed.
    #[error("unlock failed")]
    Unlock,
    /// Session expired or is invalid.
    #[error("session expired or invalid")]
    Session,
    /// IPC validation failed.
    #[error("ipc validation failed")]
    Ipc,
    /// Device is not trusted.
    #[error("device is not trusted")]
    DeviceTrust,
    /// Autofill request is not allowed.
    #[error("autofill request denied")]
    AutofillDenied,
    /// A panic was caught at a trust boundary.
    #[error("internal panic caught at trust boundary")]
    InternalPanic,
    /// A runtime invariant was violated.
    #[error("runtime integrity invariant violated")]
    IntegrityViolation,
}

impl From<espass_crypto_core::CryptoError> for RuntimeError {
    fn from(_: espass_crypto_core::CryptoError) -> Self {
        Self::Crypto
    }
}
