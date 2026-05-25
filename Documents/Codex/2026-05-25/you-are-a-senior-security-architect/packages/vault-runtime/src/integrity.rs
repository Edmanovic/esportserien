//! Runtime invariant checks that must pass before any secret material is exposed.
//!
//! Call [`check_runtime_invariants`] at the entry point of every function that
//! touches decrypted vault material. Fail closed on any violation.

use time::OffsetDateTime;

use crate::{RuntimeError, RuntimeSecretStore, SessionRuntime};

/// Checks all runtime invariants required before secret material may be accessed.
///
/// Returns `Err(RuntimeError::Session)` when the session has expired.
/// Returns `Err(RuntimeError::IntegrityViolation)` when the vault is locked.
pub fn check_runtime_invariants(
    secrets: &RuntimeSecretStore,
    session: &SessionRuntime,
    now: OffsetDateTime,
) -> Result<(), RuntimeError> {
    if session.is_expired(now) {
        return Err(RuntimeError::Session);
    }
    if !secrets.is_unlocked() {
        return Err(RuntimeError::IntegrityViolation);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RuntimeSecretStore, SessionRuntime};
    use espass_crypto_core::VaultKey;
    use time::OffsetDateTime;
    use uuid::Uuid;

    #[test]
    fn invariants_fail_when_store_locked() {
        let secrets = RuntimeSecretStore::locked();
        let now = OffsetDateTime::now_utc();
        let session = SessionRuntime::new(now, 900, 43_200);
        let result = check_runtime_invariants(&secrets, &session, now);
        assert_eq!(result, Err(crate::RuntimeError::IntegrityViolation));
    }

    #[test]
    fn invariants_fail_when_session_expired() {
        use time::Duration;
        let mut secrets = RuntimeSecretStore::locked();
        let vault_id = Uuid::new_v4();
        let key = VaultKey::from_bytes([7u8; 32]);
        secrets.open(vault_id, key).unwrap();
        let past = OffsetDateTime::now_utc() - Duration::seconds(10_000);
        let session = SessionRuntime::new(past, 1, 1);
        let now = OffsetDateTime::now_utc();
        let result = check_runtime_invariants(&secrets, &session, now);
        assert_eq!(result, Err(crate::RuntimeError::Session));
    }

    #[test]
    fn invariants_pass_when_unlocked_and_active() {
        let mut secrets = RuntimeSecretStore::locked();
        let vault_id = Uuid::new_v4();
        let key = VaultKey::from_bytes([5u8; 32]);
        secrets.open(vault_id, key).unwrap();
        let now = OffsetDateTime::now_utc();
        let session = SessionRuntime::new(now, 900, 43_200);
        assert!(check_runtime_invariants(&secrets, &session, now).is_ok());
    }
}
