//! Panic boundary isolation and cross-boundary error sanitization.
//!
//! Every Tauri command and IPC handler must wrap its body in `catch_vault_panic`
//! and map its result through `sanitize_error` before returning to the renderer.
//! This prevents panic unwinding across the FFI boundary and prevents internal
//! error detail from leaking to untrusted callers.

use std::panic::{self, AssertUnwindSafe};

use serde::{Deserialize, Serialize};

use crate::RuntimeError;

/// Runs `f` and converts any Rust panic into [`RuntimeError::InternalPanic`].
///
/// Use this at every Tauri command boundary and native messaging handler entry
/// point to prevent panics from propagating across trust boundaries.
pub fn catch_vault_panic<F, T>(f: F) -> Result<T, RuntimeError>
where
    F: FnOnce() -> Result<T, RuntimeError>,
{
    // AssertUnwindSafe is required because RuntimeSecretStore contains raw
    // pointers. The closure must never violate the invariants of those types
    // after a panic — callers must treat a caught panic as fatal and lock the
    // vault before retrying.
    panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or(Err(RuntimeError::InternalPanic))
}

/// Error variants safe to return across IPC, Tauri, or HTTP trust boundaries.
///
/// These are intentionally coarse to prevent information leakage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, thiserror::Error)]
#[serde(rename_all = "kebab-case")]
pub enum SanitizedError {
    /// The caller supplied invalid credentials or failed authentication.
    #[error("authentication failed")]
    AuthFailed,
    /// The caller's session has expired and must be re-established.
    #[error("session expired")]
    SessionExpired,
    /// The requested operation is not permitted for this caller.
    #[error("operation denied")]
    OperationDenied,
    /// An internal error occurred; no detail is disclosed to the caller.
    #[error("internal error")]
    InternalError,
}

/// Maps a [`RuntimeError`] to its coarse [`SanitizedError`] equivalent.
///
/// Call this before returning any error to an untrusted caller. Internal errors
/// (crypto, persistence, integrity, migration, panic) all collapse to
/// `InternalError` to avoid oracle attacks.
#[must_use]
pub fn sanitize_error(error: RuntimeError) -> SanitizedError {
    match error {
        RuntimeError::Unlock => SanitizedError::AuthFailed,
        RuntimeError::Session => SanitizedError::SessionExpired,
        RuntimeError::Ipc
        | RuntimeError::DeviceTrust
        | RuntimeError::AutofillDenied => SanitizedError::OperationDenied,
        RuntimeError::Crypto
        | RuntimeError::Persistence
        | RuntimeError::Integrity
        | RuntimeError::Migration
        | RuntimeError::InternalPanic
        | RuntimeError::IntegrityViolation => SanitizedError::InternalError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RuntimeError;

    #[test]
    fn catch_panic_converts_panic_to_internal_panic() {
        let result = catch_vault_panic(|| -> Result<(), RuntimeError> {
            panic!("simulated panic at trust boundary");
        });
        assert_eq!(result, Err(RuntimeError::InternalPanic));
    }

    #[test]
    fn catch_panic_passes_through_ok() {
        let result = catch_vault_panic(|| Ok::<u32, RuntimeError>(99));
        assert_eq!(result, Ok(99));
    }

    #[test]
    fn catch_panic_passes_through_err() {
        let result = catch_vault_panic(|| Err::<u32, RuntimeError>(RuntimeError::Unlock));
        assert_eq!(result, Err(RuntimeError::Unlock));
    }

    #[test]
    fn sanitize_unlock_maps_to_auth_failed() {
        assert_eq!(sanitize_error(RuntimeError::Unlock), SanitizedError::AuthFailed);
    }

    #[test]
    fn sanitize_session_maps_to_session_expired() {
        assert_eq!(sanitize_error(RuntimeError::Session), SanitizedError::SessionExpired);
    }

    #[test]
    fn sanitize_ipc_maps_to_operation_denied() {
        assert_eq!(sanitize_error(RuntimeError::Ipc), SanitizedError::OperationDenied);
    }

    #[test]
    fn sanitize_autofill_denied_maps_to_operation_denied() {
        assert_eq!(
            sanitize_error(RuntimeError::AutofillDenied),
            SanitizedError::OperationDenied,
        );
    }

    #[test]
    fn sanitize_crypto_maps_to_internal() {
        assert_eq!(sanitize_error(RuntimeError::Crypto), SanitizedError::InternalError);
    }

    #[test]
    fn sanitize_internal_panic_maps_to_internal() {
        assert_eq!(
            sanitize_error(RuntimeError::InternalPanic),
            SanitizedError::InternalError,
        );
    }

    #[test]
    fn sanitized_error_is_serializable() {
        let json = serde_json::to_string(&SanitizedError::AuthFailed).unwrap();
        assert_eq!(json, r#""auth-failed""#);
    }
}
