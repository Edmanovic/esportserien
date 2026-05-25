//! Secure unlock lifecycle and runtime secret storage.

use espass_crypto_core::{
    decrypt_with_master_key, derive_master_key_from_buffer, EncryptedEnvelope, KdfParams,
    MemoryLock, MemoryLockGuard, Salt, SecureBuffer, VaultKey,
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::RuntimeError;

/// Runtime-only secret store for an unlocked vault.
pub struct RuntimeSecretStore {
    vault_lock: Option<MemoryLockGuard>,
    vault_key: Option<VaultKey>,
    vault_id: Option<Uuid>,
}

impl RuntimeSecretStore {
    /// Creates a locked secret store.
    #[must_use]
    pub fn locked() -> Self {
        Self {
            vault_lock: None,
            vault_key: None,
            vault_id: None,
        }
    }

    /// Opens the runtime with a vault key.
    pub fn open(&mut self, vault_id: Uuid, vault_key: VaultKey) -> Result<(), RuntimeError> {
        self.vault_key = Some(vault_key);
        self.vault_lock = self
            .vault_key
            .as_mut()
            .and_then(|key| key.lock_memory().ok());
        self.vault_id = Some(vault_id);
        Ok(())
    }

    /// Returns the unlocked vault key.
    pub fn vault_key(&self) -> Result<&VaultKey, RuntimeError> {
        self.vault_key.as_ref().ok_or(RuntimeError::Unlock)
    }

    /// Locks and wipes runtime secrets.
    pub fn lock(&mut self) {
        self.vault_lock = None;
        self.vault_key = None;
        self.vault_id = None;
    }

    /// Returns whether the store is unlocked.
    #[must_use]
    pub fn is_unlocked(&self) -> bool {
        self.vault_key.is_some()
    }

    /// Returns true when the vault key's memory pages are pinned by the OS.
    ///
    /// This may return `false` even after a successful `open()` if `mlock`/`VirtualLock`
    /// failed due to process limits or sandbox restrictions. Use [`RuntimeSecretStore::is_unlocked`]
    /// to check whether the vault key is available, regardless of lock state.
    #[must_use]
    pub fn memory_lock_active(&self) -> bool {
        self.vault_lock.is_some()
    }

    /// Returns the vault UUID when unlocked.
    #[must_use]
    pub fn vault_id(&self) -> Option<Uuid> {
        self.vault_id
    }
}

/// Active unlock/session runtime.
#[derive(Debug, Clone)]
pub struct SessionRuntime {
    /// Session identifier.
    pub session_id: Uuid,
    /// Session creation timestamp.
    pub created_at: OffsetDateTime,
    /// Last activity timestamp.
    pub last_activity_at: OffsetDateTime,
    /// Idle expiry timestamp.
    pub idle_expires_at: OffsetDateTime,
    /// Absolute expiry timestamp.
    pub absolute_expires_at: OffsetDateTime,
}

impl SessionRuntime {
    /// Creates a runtime session.
    #[must_use]
    pub fn new(now: OffsetDateTime, idle_seconds: i64, absolute_seconds: i64) -> Self {
        Self {
            session_id: Uuid::new_v4(),
            created_at: now,
            last_activity_at: now,
            idle_expires_at: now + Duration::seconds(idle_seconds),
            absolute_expires_at: now + Duration::seconds(absolute_seconds),
        }
    }

    /// Records activity and extends idle expiry if absolute expiry permits.
    pub fn touch(&mut self, now: OffsetDateTime, idle_seconds: i64) -> Result<(), RuntimeError> {
        if self.is_expired(now) {
            return Err(RuntimeError::Session);
        }
        self.last_activity_at = now;
        let proposed = now + Duration::seconds(idle_seconds);
        self.idle_expires_at = proposed.min(self.absolute_expires_at);
        Ok(())
    }

    /// Returns true when idle or absolute expiry has elapsed.
    #[must_use]
    pub fn is_expired(&self, now: OffsetDateTime) -> bool {
        now >= self.idle_expires_at || now >= self.absolute_expires_at
    }
}

/// Auto-lock policy engine.
#[derive(Debug, Clone, Copy)]
pub struct AutoLockEngine {
    /// Idle timeout in seconds.
    pub idle_seconds: i64,
    /// Absolute timeout in seconds.
    pub absolute_seconds: i64,
}

impl Default for AutoLockEngine {
    fn default() -> Self {
        Self {
            idle_seconds: 900,
            absolute_seconds: 43_200,
        }
    }
}

impl AutoLockEngine {
    /// Locks secrets if the session expired.
    pub fn enforce(
        &self,
        session: &SessionRuntime,
        secrets: &mut RuntimeSecretStore,
        now: OffsetDateTime,
    ) -> Result<(), RuntimeError> {
        if session.is_expired(now) {
            secrets.lock();
            Err(RuntimeError::Session)
        } else {
            Ok(())
        }
    }
}

/// Unlock manager with throttling.
#[derive(Debug, Clone)]
pub struct UnlockManager {
    kdf_params: KdfParams,
    failed_attempts: u32,
    locked_until: Option<OffsetDateTime>,
}

impl UnlockManager {
    /// Creates an unlock manager.
    #[must_use]
    pub fn new(kdf_params: KdfParams) -> Self {
        Self {
            kdf_params,
            failed_attempts: 0,
            locked_until: None,
        }
    }

    /// Derives the master key, decrypts the vault key, and establishes runtime state.
    pub fn unlock(
        &mut self,
        password: &mut SecureBuffer,
        salt: &Salt,
        encrypted_vault_key: &EncryptedEnvelope,
        vault_id: Uuid,
        now: OffsetDateTime,
        secrets: &mut RuntimeSecretStore,
    ) -> Result<SessionRuntime, RuntimeError> {
        if self.locked_until.is_some_and(|until| now < until) {
            return Err(RuntimeError::Unlock);
        }

        let master_key = derive_master_key_from_buffer(password, salt, self.kdf_params)
            .map_err(|_| self.record_failure(now))?;
        let key_bytes = decrypt_with_master_key(
            &master_key,
            encrypted_vault_key,
            format!("espass:vault-key:v1:{vault_id}").as_bytes(),
        )
        .map_err(|_| self.record_failure(now))?;
        let vault_key = buffer_to_vault_key(&key_bytes).map_err(|_| self.record_failure(now))?;
        secrets.open(vault_id, vault_key)?;
        self.failed_attempts = 0;
        self.locked_until = None;

        Ok(SessionRuntime::new(now, 900, 43_200))
    }

    fn record_failure(&mut self, now: OffsetDateTime) -> RuntimeError {
        self.failed_attempts = self.failed_attempts.saturating_add(1);
        let delay = 2_i64.pow(self.failed_attempts.min(6));
        self.locked_until = Some(now + Duration::seconds(delay));
        RuntimeError::Unlock
    }
}

fn buffer_to_vault_key(buffer: &SecureBuffer) -> Result<VaultKey, RuntimeError> {
    if buffer.len() != 32 {
        return Err(RuntimeError::Unlock);
    }
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(buffer.expose_secret());
    Ok(VaultKey::from_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use espass_crypto_core::VaultKey;
    use uuid::Uuid;

    #[test]
    fn memory_lock_active_false_when_locked() {
        let store = RuntimeSecretStore::locked();
        assert!(!store.memory_lock_active());
    }

    #[test]
    fn vault_id_none_when_locked() {
        let store = RuntimeSecretStore::locked();
        assert!(store.vault_id().is_none());
    }

    #[test]
    fn vault_id_some_after_open() {
        let mut store = RuntimeSecretStore::locked();
        let id = Uuid::new_v4();
        let key = VaultKey::from_bytes([1u8; 32]);
        store.open(id, key).unwrap();
        assert_eq!(store.vault_id(), Some(id));
    }

    #[test]
    fn locking_clears_memory_lock_and_vault_id() {
        let mut store = RuntimeSecretStore::locked();
        let id = Uuid::new_v4();
        let key = VaultKey::from_bytes([2u8; 32]);
        store.open(id, key).unwrap();
        assert!(store.is_unlocked());
        store.lock();
        assert!(!store.memory_lock_active());
        assert!(store.vault_id().is_none());
        assert!(!store.is_unlocked());
    }
}
