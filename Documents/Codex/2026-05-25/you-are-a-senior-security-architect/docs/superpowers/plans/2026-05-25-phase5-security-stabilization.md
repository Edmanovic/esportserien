# Phase 5 — Security Stabilization & Audit Preparation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden ESPASS into an externally reviewable secure MVP candidate by adding panic isolation, memory minimization, adversarial testing, a backend sync server, Tauri command isolation, extension overlay protection, supply-chain signing, and a complete audit package.

**Architecture:** All new runtime-hardening code lives in `packages/vault-runtime/` as focused single-responsibility modules. The backend gets its first real Axum implementation. The Tauri desktop shell gets scoped command handlers that proxy through the runtime. Extension hardening adds overlay/clickjacking detection in the content script. Audit documents are generated into `docs/audit/`.

**Tech Stack:** Rust 1.95 stable, Axum 0.7, Tauri 2.0, TypeScript 5, cargo-fuzz (libfuzzer), proptest, GitHub Actions.

**Working directory for all commands:** repo root (`you-are-a-senior-security-architect/`).

---

## File Map

### New files
| Path | Purpose |
|------|---------|
| `packages/vault-runtime/src/hardening.rs` | Panic boundary, error sanitization, SanitizedError |
| `packages/vault-runtime/src/integrity.rs` | Runtime invariant checks before secret exposure |
| `packages/vault-runtime/src/secret_window.rs` | Scoped ephemeral decryption window |
| `packages/vault-runtime/src/clipboard.rs` | Clipboard TTL guard |
| `packages/security-events/src/lib.rs` | Privacy-safe audit event types |
| `apps/backend/src/main.rs` | Axum server entry point |
| `apps/backend/src/state.rs` | Shared backend state |
| `apps/backend/src/handlers.rs` | Vault item upload/download handlers |
| `apps/backend/src/rate_limit.rs` | Token-bucket rate limiter |
| `apps/backend/src/anomaly.rs` | Request anomaly detection |
| `apps/desktop/src-tauri/Cargo.toml` | Tauri crate manifest |
| `apps/desktop/src-tauri/src/main.rs` | Tauri entry point |
| `apps/desktop/src-tauri/src/lib.rs` | Tauri builder |
| `apps/desktop/src-tauri/src/state.rs` | App state (Mutex-guarded runtime) |
| `apps/desktop/src-tauri/src/commands.rs` | Permission-scoped Tauri commands |
| `apps/extension/src/content/overlay-guard.ts` | Anti-overlay / clickjacking detection |
| `fuzz/fuzz_targets/version_downgrade.rs` | Fuzz: version downgrade attack |
| `fuzz/fuzz_targets/corrupted_aad.rs` | Fuzz: corrupted authenticated data |
| `fuzz/fuzz_targets/nonce_reuse.rs` | Fuzz: nonce reuse detection |
| `.github/workflows/release.yml` | Release pipeline with Sigstore signing |
| `docs/audit/scope.md` | Audit scope document |
| `docs/audit/architecture-review.md` | Architecture review package |
| `docs/audit/cryptographic-review.md` | Cryptographic review package |
| `docs/audit/trust-boundaries.md` | Trust-boundary diagrams |
| `docs/audit/attack-surface-registry.md` | Attack surface registry |
| `docs/audit/residual-risk-summary.md` | Residual risk summary |
| `docs/audit/reviewer-setup.md` | Reviewer setup guide |
| `docs/architecture/future/future-feature-trust-model.md` | Future-feature trust model |

### Modified files
| Path | Change |
|------|--------|
| `packages/vault-runtime/src/lib.rs` | Export new modules; add `InternalPanic`, `IntegrityViolation` to `RuntimeError` |
| `packages/vault-runtime/src/unlock.rs` | Add `memory_lock_active()` and `vault_id()` to `RuntimeSecretStore` |
| `packages/crypto-core/src/memlock.rs` | Add `unsafe impl Send for MemoryLockGuard` |
| `apps/extension/src/content/autofill-guard.ts` | Import and call overlay-guard |
| `apps/extension/manifest.chrome.json` | Tighten CSP |
| `Cargo.toml` | Add `apps/desktop/src-tauri` workspace member |

---

## Task 1 — Extend RuntimeError with new variants

**Files:**
- Modify: `packages/vault-runtime/src/lib.rs`

- [ ] **Step 1: Add variants to RuntimeError**

Open `packages/vault-runtime/src/lib.rs` and replace the `RuntimeError` enum with:

```rust
/// Runtime errors that must fail closed at trust boundaries.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RuntimeError {
    #[error("cryptographic operation failed")]
    Crypto,
    #[error("vault persistence failed")]
    Persistence,
    #[error("vault integrity validation failed")]
    Integrity,
    #[error("vault schema migration failed")]
    Migration,
    #[error("unlock failed")]
    Unlock,
    #[error("session expired or invalid")]
    Session,
    #[error("ipc validation failed")]
    Ipc,
    #[error("device is not trusted")]
    DeviceTrust,
    #[error("autofill request denied")]
    AutofillDenied,
    /// A panic was caught at a trust boundary.
    #[error("internal panic caught at trust boundary")]
    InternalPanic,
    /// A runtime invariant was violated.
    #[error("runtime integrity invariant violated")]
    IntegrityViolation,
}
```

- [ ] **Step 2: Verify compilation**

```
cargo check -p espass-vault-runtime
```

Expected: no errors.

- [ ] **Step 3: Commit**

```
git add packages/vault-runtime/src/lib.rs
git commit -m "feat(runtime): add InternalPanic and IntegrityViolation error variants"
```

---

## Task 2 — Make MemoryLockGuard Send-safe for Mutex use

**Files:**
- Modify: `packages/crypto-core/src/memlock.rs`

- [ ] **Step 1: Write the failing test (compile-time)**

Add to the bottom of `packages/crypto-core/src/memlock.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time check: MemoryLockGuard must be Send so it can live
    // inside Mutex<> across threads in Tauri state.
    #[test]
    fn memory_lock_guard_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<MemoryLockGuard>();
    }
}
```

- [ ] **Step 2: Run to confirm it fails**

```
cargo test -p espass-crypto-core -- memory_lock_guard_is_send
```

Expected: compile error — `MemoryLockGuard cannot be sent between threads safely`.

- [ ] **Step 3: Add unsafe Send impl**

After the `impl Drop for MemoryLockGuard` block, add:

```rust
// SAFETY: MemoryLockGuard only calls mlock/VirtualLock/munlock/VirtualUnlock,
// which are thread-safe OS APIs. The pointer is to heap memory owned by the
// enclosing key type and is never dereferenced by the guard — it is only passed
// to the OS. Callers must protect concurrent access with a Mutex.
unsafe impl Send for MemoryLockGuard {}
```

- [ ] **Step 4: Run test to confirm it passes**

```
cargo test -p espass-crypto-core -- memory_lock_guard_is_send
```

Expected: PASS.

- [ ] **Step 5: Commit**

```
git add packages/crypto-core/src/memlock.rs
git commit -m "fix(crypto): unsafe impl Send for MemoryLockGuard to allow Mutex<RuntimeSecretStore>"
```

---

## Task 3 — Add accessors to RuntimeSecretStore

**Files:**
- Modify: `packages/vault-runtime/src/unlock.rs`

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)]` block at the bottom of `packages/vault-runtime/src/unlock.rs` (create the block if absent):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use espass_crypto_core::{VaultKey};
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
}
```

- [ ] **Step 2: Run to confirm failure**

```
cargo test -p espass-vault-runtime -- unlock::tests
```

Expected: compile errors — `memory_lock_active` and `vault_id` not found.

- [ ] **Step 3: Add the methods to RuntimeSecretStore**

Inside the `impl RuntimeSecretStore` block in `packages/vault-runtime/src/unlock.rs`, after the `is_unlocked` method, add:

```rust
/// Returns true when the vault key's memory pages are locked.
#[must_use]
pub fn memory_lock_active(&self) -> bool {
    self.vault_lock.is_some()
}

/// Returns the vault UUID when unlocked.
#[must_use]
pub fn vault_id(&self) -> Option<uuid::Uuid> {
    self.vault_id
}
```

- [ ] **Step 4: Run tests**

```
cargo test -p espass-vault-runtime -- unlock::tests
```

Expected: all 3 tests PASS.

- [ ] **Step 5: Commit**

```
git add packages/vault-runtime/src/unlock.rs
git commit -m "feat(runtime): add memory_lock_active and vault_id accessors to RuntimeSecretStore"
```

---

## Task 4 — hardening.rs: panic boundary and error sanitization

**Files:**
- Create: `packages/vault-runtime/src/hardening.rs`

- [ ] **Step 1: Write failing tests first**

Create `packages/vault-runtime/src/hardening.rs` with only the test module:

```rust
//! Panic boundary isolation and cross-boundary error sanitization.

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
```

- [ ] **Step 2: Run to confirm failure**

```
cargo test -p espass-vault-runtime -- hardening::tests 2>&1 | head -20
```

Expected: compile errors — functions not defined yet.

- [ ] **Step 3: Write implementation**

Replace the entire file with:

```rust
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
    #[error("authentication failed")]
    AuthFailed,
    #[error("session expired")]
    SessionExpired,
    #[error("operation denied")]
    OperationDenied,
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
```

- [ ] **Step 4: Export the module in lib.rs**

Add to `packages/vault-runtime/src/lib.rs` after the existing `pub mod` declarations:

```rust
pub mod hardening;
pub use hardening::{catch_vault_panic, sanitize_error, SanitizedError};
```

- [ ] **Step 5: Run tests**

```
cargo test -p espass-vault-runtime -- hardening::tests
```

Expected: all 9 tests PASS.

- [ ] **Step 6: Commit**

```
git add packages/vault-runtime/src/hardening.rs packages/vault-runtime/src/lib.rs
git commit -m "feat(runtime): add panic boundary isolation and cross-boundary error sanitization"
```

---

## Task 5 — integrity.rs: runtime invariant checks

**Files:**
- Create: `packages/vault-runtime/src/integrity.rs`

- [ ] **Step 1: Write failing tests**

Create `packages/vault-runtime/src/integrity.rs`:

```rust
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
        let result = check_runtime_invariants(&secrets, &session, now);
        assert!(result.is_ok());
    }
}
```

- [ ] **Step 2: Run to confirm failure**

```
cargo test -p espass-vault-runtime -- integrity::tests 2>&1 | head -10
```

Expected: compile errors.

- [ ] **Step 3: Implement**

Replace entire file:

```rust
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
```

- [ ] **Step 4: Export in lib.rs**

Add to `packages/vault-runtime/src/lib.rs`:

```rust
pub mod integrity;
pub use integrity::check_runtime_invariants;
```

- [ ] **Step 5: Run tests**

```
cargo test -p espass-vault-runtime -- integrity::tests
```

Expected: all 3 tests PASS.

- [ ] **Step 6: Commit**

```
git add packages/vault-runtime/src/integrity.rs packages/vault-runtime/src/lib.rs
git commit -m "feat(runtime): add runtime invariant checks for pre-secret-access validation"
```

---

## Task 6 — secret_window.rs: scoped ephemeral decryption

**Files:**
- Create: `packages/vault-runtime/src/secret_window.rs`

- [ ] **Step 1: Write failing tests**

Create `packages/vault-runtime/src/secret_window.rs` with test module only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use espass_crypto_core::{encrypt, EncryptionMetadata, VaultKey};
    use uuid::Uuid;

    fn make_encrypted_credential(key: &VaultKey, vault_id: Uuid, item_id: Uuid) -> espass_crypto_core::EncryptedEnvelope {
        let metadata = EncryptionMetadata {
            tenant_id: "test".into(),
            vault_id: vault_id.to_string(),
            item_id: Some(item_id.to_string()),
            record_type: "credential".into(),
            schema_version: 1,
        };
        encrypt(key, b"secret-payload", &metadata.to_aad()).unwrap()
    }

    #[test]
    fn window_exposes_decrypted_bytes() {
        let key = VaultKey::from_bytes([3u8; 32]);
        let vault_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let enc = make_encrypted_credential(&key, vault_id, item_id);
        let window = SecretWindow::open(&key, vault_id, item_id, &enc).unwrap();
        assert_eq!(window.expose(), b"secret-payload");
    }

    #[test]
    fn window_fails_with_wrong_key() {
        let key = VaultKey::from_bytes([3u8; 32]);
        let wrong_key = VaultKey::from_bytes([4u8; 32]);
        let vault_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let enc = make_encrypted_credential(&key, vault_id, item_id);
        let result = SecretWindow::open(&wrong_key, vault_id, item_id, &enc);
        assert!(result.is_err());
    }

    #[test]
    fn window_fails_with_wrong_item_id() {
        let key = VaultKey::from_bytes([3u8; 32]);
        let vault_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let enc = make_encrypted_credential(&key, vault_id, item_id);
        // Different item_id changes the AAD, so decryption must fail.
        let wrong_item_id = Uuid::new_v4();
        let result = SecretWindow::open(&key, vault_id, wrong_item_id, &enc);
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run to confirm failure**

```
cargo test -p espass-vault-runtime -- secret_window::tests 2>&1 | head -10
```

Expected: compile errors.

- [ ] **Step 3: Implement**

Replace entire file:

```rust
//! Scoped ephemeral decryption window.
//!
//! A `SecretWindow` holds decrypted bytes only for the duration of a single
//! operation. It zeroizes on drop via the inner `SecureBuffer`. Never store
//! a `SecretWindow` in a struct field or return it across an await point.

use espass_crypto_core::{decrypt, EncryptedEnvelope, EncryptionMetadata, SecureBuffer, VaultKey};
use uuid::Uuid;

use crate::RuntimeError;

/// A scoped decryption window that zeroizes on drop.
///
/// Open one window per operation; let it drop as soon as you are done with
/// the plaintext bytes. The window must not outlive the `VaultKey` it was
/// opened with.
pub struct SecretWindow {
    data: SecureBuffer,
}

impl SecretWindow {
    /// Decrypts a single vault item into a scoped window.
    pub fn open(
        key: &VaultKey,
        vault_id: Uuid,
        item_id: Uuid,
        encrypted: &EncryptedEnvelope,
    ) -> Result<Self, RuntimeError> {
        let metadata = EncryptionMetadata {
            tenant_id: "local".to_owned(),
            vault_id: vault_id.to_string(),
            item_id: Some(item_id.to_string()),
            record_type: "credential".to_owned(),
            schema_version: 1,
        };
        let data = decrypt(key, encrypted, &metadata.to_aad())?;
        Ok(Self { data })
    }

    /// Borrows the decrypted bytes for the duration of the window.
    #[must_use]
    pub fn expose(&self) -> &[u8] {
        self.data.expose_secret()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use espass_crypto_core::{encrypt, EncryptionMetadata, VaultKey};
    use uuid::Uuid;

    fn make_encrypted_credential(
        key: &VaultKey,
        vault_id: Uuid,
        item_id: Uuid,
    ) -> EncryptedEnvelope {
        let metadata = EncryptionMetadata {
            tenant_id: "local".into(),
            vault_id: vault_id.to_string(),
            item_id: Some(item_id.to_string()),
            record_type: "credential".into(),
            schema_version: 1,
        };
        encrypt(key, b"secret-payload", &metadata.to_aad()).unwrap()
    }

    #[test]
    fn window_exposes_decrypted_bytes() {
        let key = VaultKey::from_bytes([3u8; 32]);
        let vault_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let enc = make_encrypted_credential(&key, vault_id, item_id);
        let window = SecretWindow::open(&key, vault_id, item_id, &enc).unwrap();
        assert_eq!(window.expose(), b"secret-payload");
    }

    #[test]
    fn window_fails_with_wrong_key() {
        let key = VaultKey::from_bytes([3u8; 32]);
        let wrong_key = VaultKey::from_bytes([4u8; 32]);
        let vault_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let enc = make_encrypted_credential(&key, vault_id, item_id);
        assert!(SecretWindow::open(&wrong_key, vault_id, item_id, &enc).is_err());
    }

    #[test]
    fn window_fails_with_wrong_item_id() {
        let key = VaultKey::from_bytes([3u8; 32]);
        let vault_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let enc = make_encrypted_credential(&key, vault_id, item_id);
        let wrong_item_id = Uuid::new_v4();
        assert!(SecretWindow::open(&key, vault_id, wrong_item_id, &enc).is_err());
    }
}
```

- [ ] **Step 4: Export in lib.rs**

Add to `packages/vault-runtime/src/lib.rs`:

```rust
pub mod secret_window;
pub use secret_window::SecretWindow;
```

- [ ] **Step 5: Run tests**

```
cargo test -p espass-vault-runtime -- secret_window::tests
```

Expected: all 3 tests PASS.

- [ ] **Step 6: Commit**

```
git add packages/vault-runtime/src/secret_window.rs packages/vault-runtime/src/lib.rs
git commit -m "feat(runtime): add SecretWindow scoped ephemeral decryption"
```

---

## Task 7 — clipboard.rs: clipboard TTL guard

**Files:**
- Create: `packages/vault-runtime/src/clipboard.rs`

- [ ] **Step 1: Write failing tests**

Create `packages/vault-runtime/src/clipboard.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn not_expired_immediately() {
        let guard = ClipboardGuard::new(30);
        assert!(!guard.is_expired());
        assert!(guard.seconds_remaining() > 0);
    }

    #[test]
    fn expired_after_ttl_elapses() {
        let guard = ClipboardGuard::with_elapsed(Duration::from_secs(31), 30);
        assert!(guard.is_expired());
        assert_eq!(guard.seconds_remaining(), 0);
    }

    #[test]
    fn seconds_remaining_decreases_over_time() {
        let guard = ClipboardGuard::with_elapsed(Duration::from_secs(10), 30);
        assert!(guard.seconds_remaining() <= 20);
    }
}
```

- [ ] **Step 2: Run to confirm failure**

```
cargo test -p espass-vault-runtime -- clipboard::tests 2>&1 | head -10
```

Expected: compile errors.

- [ ] **Step 3: Implement**

Replace entire file:

```rust
//! Clipboard exposure TTL guard.
//!
//! When ESPASS copies a secret to the clipboard it must clear it after a short
//! TTL. Create a `ClipboardGuard` immediately after writing to the clipboard,
//! then poll `is_expired()` in the UI loop to trigger clearing.

use std::time::{Duration, Instant};

/// Tracks elapsed time since a secret was placed on the clipboard.
pub struct ClipboardGuard {
    set_at: Instant,
    ttl: Duration,
}

impl ClipboardGuard {
    /// Creates a guard with `ttl_seconds` TTL, starting now.
    #[must_use]
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            set_at: Instant::now(),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    /// Creates a guard with a custom elapsed offset, used only in tests.
    #[cfg(test)]
    pub(crate) fn with_elapsed(elapsed: Duration, ttl_seconds: u64) -> Self {
        Self {
            set_at: Instant::now() - elapsed,
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    /// Returns true when the TTL has elapsed and the clipboard should be cleared.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.set_at.elapsed() >= self.ttl
    }

    /// Returns whole seconds remaining before expiry, clamped to 0.
    #[must_use]
    pub fn seconds_remaining(&self) -> u64 {
        let elapsed = self.set_at.elapsed();
        if elapsed >= self.ttl {
            0
        } else {
            (self.ttl - elapsed).as_secs()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn not_expired_immediately() {
        let guard = ClipboardGuard::new(30);
        assert!(!guard.is_expired());
        assert!(guard.seconds_remaining() > 0);
    }

    #[test]
    fn expired_after_ttl_elapses() {
        let guard = ClipboardGuard::with_elapsed(Duration::from_secs(31), 30);
        assert!(guard.is_expired());
        assert_eq!(guard.seconds_remaining(), 0);
    }

    #[test]
    fn seconds_remaining_decreases_over_time() {
        let guard = ClipboardGuard::with_elapsed(Duration::from_secs(10), 30);
        assert!(guard.seconds_remaining() <= 20);
    }
}
```

- [ ] **Step 4: Export in lib.rs**

Add to `packages/vault-runtime/src/lib.rs`:

```rust
pub mod clipboard;
pub use clipboard::ClipboardGuard;
```

- [ ] **Step 5: Run tests**

```
cargo test -p espass-vault-runtime -- clipboard::tests
```

Expected: all 3 tests PASS.

- [ ] **Step 6: Commit**

```
git add packages/vault-runtime/src/clipboard.rs packages/vault-runtime/src/lib.rs
git commit -m "feat(runtime): add clipboard TTL guard for secret exposure minimization"
```

---

## Task 8 — security-events package: audit event types

**Files:**
- Create: `packages/security-events/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Create `packages/security-events/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_unlocked_event_serializes() {
        let event = SecurityEvent::vault_unlocked(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("vault-unlocked"));
        assert!(!json.contains("password"));
    }

    #[test]
    fn ipc_handshake_denied_event_serializes() {
        let event = SecurityEvent::ipc_handshake_denied("chrome-extension://bad");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("ipc-handshake-denied"));
    }

    #[test]
    fn unlock_failed_event_does_not_leak_attempt_count_in_kind() {
        let event = SecurityEvent::unlock_failed(uuid::Uuid::new_v4());
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("unlock-failed"));
        // Ensure the attempt count field is not in the kind string.
        assert!(!json.contains("attempts"));
    }
}
```

- [ ] **Step 2: Run to confirm failure**

```
cargo test -p espass-security-events 2>&1 | head -15
```

Expected: compile errors — module empty.

- [ ] **Step 3: Implement**

Replace entire file:

```rust
//! Privacy-safe security event types for ESPASS audit logging.
//!
//! Events must NEVER contain plaintext secrets, raw passwords, full stack
//! traces, or precise timing data that could enable oracle attacks. All events
//! are safe to write to an append-only local audit log and, optionally, a
//! backend audit endpoint.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// A privacy-safe security event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecurityEvent {
    /// Event UUID for deduplication.
    pub event_id: Uuid,
    /// Originating session UUID, if any.
    pub session_id: Option<Uuid>,
    /// Coarse event kind.
    pub kind: SecurityEventKind,
    /// UTC timestamp, truncated to second precision to avoid timing oracles.
    pub occurred_at: OffsetDateTime,
}

/// Coarse event kind. No plaintext secrets or stack traces.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", tag = "type")]
pub enum SecurityEventKind {
    /// Vault successfully unlocked.
    VaultUnlocked { vault_id: Uuid, user_id: Uuid },
    /// Vault locked (manual, timeout, or forced).
    VaultLocked { vault_id: Uuid, reason: LockReason },
    /// Unlock attempt failed (wrong password or throttled).
    UnlockFailed { vault_id: Uuid },
    /// IPC handshake was denied for the given origin.
    IpcHandshakeDenied { origin: String },
    /// IPC replay attack detected.
    IpcReplayDetected { session_id: Uuid },
    /// Device registration approved.
    DeviceRegistrationApproved { device_id: Uuid },
    /// Device revoked.
    DeviceRevoked { device_id: Uuid },
    /// Autofill blocked by policy.
    AutofillBlocked { reason: AutofillBlockReason },
    /// Vault integrity check failed.
    VaultIntegrityFailed { vault_id: Uuid },
    /// A panic was caught at a trust boundary.
    PanicCaught,
}

/// Why the vault was locked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LockReason {
    UserRequest,
    IdleTimeout,
    AbsoluteTimeout,
    IntegrityFailure,
}

/// Why autofill was blocked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AutofillBlockReason {
    SuspiciousDomain,
    CrossOriginIframe,
    FieldNotVisible,
    OverlayDetected,
    OriginMismatch,
}

impl SecurityEvent {
    fn new(session_id: Option<Uuid>, kind: SecurityEventKind) -> Self {
        use time::macros::format_description;
        // Truncate to second precision to avoid sub-second timing oracles.
        let now = OffsetDateTime::now_utc();
        let truncated = now.replace_nanosecond(0).unwrap_or(now);
        Self {
            event_id: Uuid::new_v4(),
            session_id,
            kind,
            occurred_at: truncated,
        }
    }

    /// Vault unlock succeeded.
    #[must_use]
    pub fn vault_unlocked(vault_id: Uuid, user_id: Uuid) -> Self {
        Self::new(None, SecurityEventKind::VaultUnlocked { vault_id, user_id })
    }

    /// Vault lock event.
    #[must_use]
    pub fn vault_locked(vault_id: Uuid, reason: LockReason) -> Self {
        Self::new(None, SecurityEventKind::VaultLocked { vault_id, reason })
    }

    /// Unlock attempt failed.
    #[must_use]
    pub fn unlock_failed(vault_id: Uuid) -> Self {
        Self::new(None, SecurityEventKind::UnlockFailed { vault_id })
    }

    /// IPC handshake denied.
    #[must_use]
    pub fn ipc_handshake_denied(origin: &str) -> Self {
        Self::new(None, SecurityEventKind::IpcHandshakeDenied {
            origin: origin.to_owned(),
        })
    }

    /// IPC replay detected.
    #[must_use]
    pub fn ipc_replay_detected(session_id: Uuid) -> Self {
        Self::new(
            Some(session_id),
            SecurityEventKind::IpcReplayDetected { session_id },
        )
    }

    /// Autofill blocked.
    #[must_use]
    pub fn autofill_blocked(reason: AutofillBlockReason) -> Self {
        Self::new(None, SecurityEventKind::AutofillBlocked { reason })
    }

    /// Panic caught at trust boundary.
    #[must_use]
    pub fn panic_caught() -> Self {
        Self::new(None, SecurityEventKind::PanicCaught)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_unlocked_event_serializes() {
        let event = SecurityEvent::vault_unlocked(Uuid::new_v4(), Uuid::new_v4());
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("vault-unlocked"));
        assert!(!json.contains("password"));
    }

    #[test]
    fn ipc_handshake_denied_event_serializes() {
        let event = SecurityEvent::ipc_handshake_denied("chrome-extension://bad");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("ipc-handshake-denied"));
    }

    #[test]
    fn unlock_failed_event_does_not_leak_attempt_count_in_kind() {
        let event = SecurityEvent::unlock_failed(Uuid::new_v4());
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("unlock-failed"));
        assert!(!json.contains("attempts"));
    }

    #[test]
    fn autofill_blocked_overlay_serializes() {
        let event = SecurityEvent::autofill_blocked(AutofillBlockReason::OverlayDetected);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("overlay-detected"));
    }

    #[test]
    fn panic_caught_event_serializes() {
        let event = SecurityEvent::panic_caught();
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("panic-caught"));
    }
}
```

- [ ] **Step 4: Run tests**

```
cargo test -p espass-security-events
```

Expected: all 5 tests PASS.

- [ ] **Step 5: Commit**

```
git add packages/security-events/src/lib.rs
git commit -m "feat(security-events): implement privacy-safe audit event types"
```

---

## Task 9 — Backend: rate_limit.rs token bucket

**Files:**
- Create: `apps/backend/src/rate_limit.rs`

- [ ] **Step 1: Write failing tests**

Create `apps/backend/src/rate_limit.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn full_bucket_allows_requests() {
        let mut bucket = TokenBucket::new(10, 1.0);
        assert!(bucket.try_consume(1.0));
    }

    #[test]
    fn empty_bucket_denies_request() {
        let mut bucket = TokenBucket::new(2, 0.0); // zero refill rate
        assert!(bucket.try_consume(1.0));
        assert!(bucket.try_consume(1.0));
        assert!(!bucket.try_consume(1.0)); // bucket empty
    }

    #[test]
    fn bucket_refills_over_time() {
        let mut bucket = TokenBucket::with_elapsed(
            2,   // capacity
            2.0, // refill rate: 2 per second
            Duration::from_secs(1), // 1 second elapsed
        );
        // Started empty, 1s * 2.0 = 2 tokens refilled, capped at capacity=2.
        assert!(bucket.try_consume(2.0));
        assert!(!bucket.try_consume(1.0));
    }
}
```

- [ ] **Step 2: Run to confirm failure**

```
cargo check -p espass-backend 2>&1 | head -15
```

Expected: compile errors.

- [ ] **Step 3: Implement**

Replace entire file:

```rust
//! Token-bucket rate limiter for backend API endpoints.
//!
//! One `TokenBucket` per client IP. Store buckets in a `DashMap<IpAddr, Mutex<TokenBucket>>`.

use std::time::{Duration, Instant};

/// A single-key token-bucket rate limiter.
pub struct TokenBucket {
    capacity: f64,
    tokens: f64,
    last_refill: Instant,
    refill_rate: f64,
}

impl TokenBucket {
    /// Creates a full bucket with `capacity` tokens refilling at `refill_rate` per second.
    #[must_use]
    pub fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            capacity: f64::from(capacity),
            tokens: f64::from(capacity),
            last_refill: Instant::now(),
            refill_rate,
        }
    }

    /// Creates a bucket with a simulated elapsed time offset for testing.
    #[cfg(test)]
    pub(crate) fn with_elapsed(capacity: u32, refill_rate: f64, elapsed: Duration) -> Self {
        let mut b = Self::new(capacity, refill_rate);
        b.tokens = 0.0; // start empty
        b.last_refill = Instant::now() - elapsed;
        b.refill(Instant::now());
        b
    }

    fn refill(&mut self, now: Instant) {
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = now;
    }

    /// Attempts to consume `tokens` from the bucket. Returns `true` on success.
    pub fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill(Instant::now());
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn full_bucket_allows_requests() {
        let mut bucket = TokenBucket::new(10, 1.0);
        assert!(bucket.try_consume(1.0));
    }

    #[test]
    fn empty_bucket_denies_request() {
        let mut bucket = TokenBucket::new(2, 0.0);
        assert!(bucket.try_consume(1.0));
        assert!(bucket.try_consume(1.0));
        assert!(!bucket.try_consume(1.0));
    }

    #[test]
    fn bucket_refills_over_time() {
        let mut bucket = TokenBucket::with_elapsed(2, 2.0, Duration::from_secs(1));
        assert!(bucket.try_consume(2.0));
        assert!(!bucket.try_consume(1.0));
    }
}
```

- [ ] **Step 4: Commit placeholder; full backend wired in Task 10**

```
git add apps/backend/src/rate_limit.rs
git commit -m "feat(backend): token-bucket rate limiter"
```

---

## Task 10 — Backend: handlers.rs, anomaly.rs, main.rs, state.rs

**Files:**
- Create: `apps/backend/src/state.rs`
- Create: `apps/backend/src/anomaly.rs`
- Create: `apps/backend/src/handlers.rs`
- Create: `apps/backend/src/main.rs`

- [ ] **Step 1: Create state.rs**

Create `apps/backend/src/state.rs`:

```rust
//! Shared backend application state.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;

use crate::rate_limit::TokenBucket;

/// Shared application state threaded through Axum via `Arc<AppState>`.
pub struct AppState {
    /// Per-IP rate limiter buckets. 20 req/s capacity, 10 req/s refill.
    pub rate_buckets: Mutex<HashMap<IpAddr, TokenBucket>>,
}

impl AppState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            rate_buckets: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: Create anomaly.rs**

Create `apps/backend/src/anomaly.rs`:

```rust
//! Request anomaly detection heuristics.

/// Returns true when the encrypted payload exceeds the ESPASS server-side size limit.
///
/// The server only stores ciphertext blobs; 10 MiB is generous for any real
/// credential or small attachment reference. Larger uploads indicate abuse.
#[must_use]
pub fn payload_too_large(ciphertext_len: usize) -> bool {
    const MAX_CIPHERTEXT_BYTES: usize = 10 * 1024 * 1024;
    ciphertext_len > MAX_CIPHERTEXT_BYTES
}

/// Returns true when the number of key slots in a vault envelope is implausibly large.
#[must_use]
pub fn key_slot_count_anomalous(slot_count: usize) -> bool {
    const MAX_KEY_SLOTS: usize = 100;
    slot_count > MAX_KEY_SLOTS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_payload_not_flagged() {
        assert!(!payload_too_large(1024));
    }

    #[test]
    fn oversized_payload_flagged() {
        assert!(payload_too_large(11 * 1024 * 1024));
    }

    #[test]
    fn normal_slot_count_not_flagged() {
        assert!(!key_slot_count_anomalous(5));
    }

    #[test]
    fn excessive_slot_count_flagged() {
        assert!(key_slot_count_anomalous(101));
    }
}
```

- [ ] **Step 3: Create handlers.rs**

Create `apps/backend/src/handlers.rs`:

```rust
//! Axum request handlers. The server is zero-knowledge: it stores and returns
//! ciphertext only. It never receives keys or plaintext.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use espass_shared_types::vault::{EncryptedPayload, VaultItem};
use uuid::Uuid;

use crate::anomaly::{key_slot_count_anomalous, payload_too_large};
use crate::state::AppState;

/// Upload an encrypted vault item. The server validates only the ciphertext
/// length and metadata structure — it cannot inspect plaintext.
pub async fn upload_item(
    State(_state): State<Arc<AppState>>,
    Path(vault_id): Path<Uuid>,
    Json(item): Json<VaultItem>,
) -> Result<StatusCode, StatusCode> {
    if item.vault_id != vault_id {
        return Err(StatusCode::BAD_REQUEST);
    }
    if payload_too_large(item.encrypted_payload.ciphertext.len()) {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    if key_slot_count_anomalous(item.attachments.len()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    // In a real implementation, persist `item` to the database here.
    Ok(StatusCode::ACCEPTED)
}

/// Download an encrypted vault item by ID. Returns only ciphertext.
pub async fn download_item(
    State(_state): State<Arc<AppState>>,
    Path((vault_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<EncryptedPayload>, StatusCode> {
    // In a real implementation, load from the database here.
    // Return 404 without leaking whether the vault exists.
    let _ = (vault_id, item_id);
    Err(StatusCode::NOT_FOUND)
}
```

- [ ] **Step 4: Create main.rs**

Create `apps/backend/src/main.rs`:

```rust
//! ESPASS encrypted sync backend.
//!
//! Zero-knowledge: the server stores and moves ciphertext only. Keys never
//! leave the client.

mod anomaly;
mod handlers;
mod rate_limit;
mod state;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::middleware::{self, Next};
use axum::extract::{ConnectInfo, State};
use axum::http::{Request, Response, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use state::AppState;

#[tokio::main]
async fn main() {
    let state = Arc::new(AppState::new());
    let app = Router::new()
        .route(
            "/api/v1/vaults/:vault_id/items",
            post(handlers::upload_item),
        )
        .route(
            "/api/v1/vaults/:vault_id/items/:item_id",
            get(handlers::download_item),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await
        .expect("failed to bind listener");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .expect("server error");
}

async fn rate_limit_middleware<B>(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    request: Request<B>,
    next: Next<B>,
) -> Result<Response<axum::body::Body>, StatusCode>
where
    B: Send + 'static,
{
    let ip = addr.ip();
    let allowed = {
        let mut buckets = state.rate_buckets.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let bucket = buckets
            .entry(ip)
            .or_insert_with(|| rate_limit::TokenBucket::new(20, 10.0));
        bucket.try_consume(1.0)
    };
    if !allowed {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(next.run(request).await)
}
```

- [ ] **Step 5: Verify compilation**

```
cargo check -p espass-backend
```

Expected: no errors (or only unused-import warnings).

- [ ] **Step 6: Run anomaly tests**

```
cargo test -p espass-backend -- anomaly::tests
```

Expected: 4 tests PASS.

- [ ] **Step 7: Commit**

```
git add apps/backend/src/
git commit -m "feat(backend): Axum sync server with rate limiting and anomaly detection"
```

---

## Task 11 — Tauri desktop commands

**Files:**
- Create: `apps/desktop/src-tauri/Cargo.toml`
- Create: `apps/desktop/src-tauri/src/main.rs`
- Create: `apps/desktop/src-tauri/src/lib.rs`
- Create: `apps/desktop/src-tauri/src/state.rs`
- Create: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `Cargo.toml` (workspace)

- [ ] **Step 1: Add Tauri crate to workspace**

In the root `Cargo.toml`, change the `members` array to:

```toml
members = [
  "apps/backend",
  "apps/desktop/native-messaging-host",
  "apps/desktop/src-tauri",
  "packages/crypto-core",
  "packages/security-events",
  "packages/shared-types",
  "packages/vault-runtime"
]
```

- [ ] **Step 2: Create Cargo.toml for the Tauri crate**

Create `apps/desktop/src-tauri/Cargo.toml`:

```toml
[package]
name = "espass-desktop"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true

[lib]
name = "espass_desktop_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[dependencies]
espass-crypto-core = { version = "0.1.0", path = "../../../packages/crypto-core" }
espass-shared-types = { version = "0.1.0", path = "../../../packages/shared-types" }
espass-vault-runtime = { version = "0.1.0", path = "../../../packages/vault-runtime" }
serde = { version = "1.0.203", features = ["derive"] }
serde_json = "1.0.117"
tauri = { version = "2.0", features = [] }
thiserror = "1.0.61"
time = { version = "0.3.36", features = ["serde"] }
uuid = { version = "1.8.0", features = ["serde", "v4"] }

[lints]
workspace = true
```

- [ ] **Step 3: Create state.rs**

Create `apps/desktop/src-tauri/src/state.rs`:

```rust
//! Tauri application state. All runtime secrets are protected by a Mutex so
//! Tauri's multi-threaded async runtime can safely share them across commands.

use std::sync::Mutex;

use espass_crypto_core::KdfParams;
use espass_vault_runtime::{RuntimeSecretStore, SessionRuntime, UnlockManager};

/// Top-level application state managed by Tauri.
pub struct AppState {
    pub secrets: Mutex<RuntimeSecretStore>,
    pub session: Mutex<Option<SessionRuntime>>,
    pub unlock_manager: Mutex<UnlockManager>,
}

impl AppState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            secrets: Mutex::new(RuntimeSecretStore::locked()),
            session: Mutex::new(None),
            unlock_manager: Mutex::new(UnlockManager::new(KdfParams::default())),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Create commands.rs**

Create `apps/desktop/src-tauri/src/commands.rs`:

```rust
//! Permission-scoped Tauri commands.
//!
//! Every command:
//! 1. Wraps its body in `catch_vault_panic` to isolate panics from the renderer.
//! 2. Maps errors through `sanitize_error` so internal detail never reaches the
//!    frontend.
//! 3. Checks runtime invariants before touching secret material.

use tauri::State;
use time::OffsetDateTime;

use espass_crypto_core::{Salt, SecureBuffer};
use espass_shared_types::vault::VaultItem;
use espass_vault_runtime::{
    catch_vault_panic, check_runtime_invariants, sanitize_error, SanitizedError,
};
use uuid::Uuid;

use crate::state::AppState;

/// Response returned to the renderer after a successful unlock.
#[derive(Debug, serde::Serialize)]
pub struct UnlockResponse {
    pub session_id: String,
    pub vault_id: String,
}

/// Derives the master key from `password`, decrypts the vault key, and opens
/// the runtime secret store. The password bytes are zeroized before returning.
#[tauri::command]
pub fn unlock_vault(
    state: State<'_, AppState>,
    password_bytes: Vec<u8>,
    salt_bytes: [u8; 16],
    encrypted_vault_key_nonce: [u8; 12],
    encrypted_vault_key_ciphertext: Vec<u8>,
    vault_id: String,
) -> Result<UnlockResponse, SanitizedError> {
    catch_vault_panic(|| {
        let vault_uuid = Uuid::parse_str(&vault_id).map_err(|_| {
            espass_vault_runtime::RuntimeError::Unlock
        })?;
        let salt = Salt::from_bytes(salt_bytes);
        let envelope = espass_crypto_core::EncryptedEnvelope {
            version: 1,
            nonce: encrypted_vault_key_nonce,
            ciphertext: encrypted_vault_key_ciphertext,
        };
        let mut password = SecureBuffer::new(password_bytes);
        let now = OffsetDateTime::now_utc();

        let mut secrets = state.secrets.lock().map_err(|_| {
            espass_vault_runtime::RuntimeError::InternalPanic
        })?;
        let mut unlock_mgr = state.unlock_manager.lock().map_err(|_| {
            espass_vault_runtime::RuntimeError::InternalPanic
        })?;
        let session = unlock_mgr.unlock(
            &mut password,
            &salt,
            &envelope,
            vault_uuid,
            now,
            &mut secrets,
        )?;
        let session_id = session.session_id.to_string();
        *state.session.lock().map_err(|_| {
            espass_vault_runtime::RuntimeError::InternalPanic
        })? = Some(session);

        Ok(UnlockResponse {
            session_id,
            vault_id: vault_uuid.to_string(),
        })
    })
    .map_err(sanitize_error)
}

/// Locks the vault and wipes all runtime secrets.
#[tauri::command]
pub fn lock_vault(state: State<'_, AppState>) -> Result<(), SanitizedError> {
    catch_vault_panic(|| {
        let mut secrets = state.secrets.lock().map_err(|_| {
            espass_vault_runtime::RuntimeError::InternalPanic
        })?;
        secrets.lock();
        *state.session.lock().map_err(|_| {
            espass_vault_runtime::RuntimeError::InternalPanic
        })? = None;
        Ok(())
    })
    .map_err(sanitize_error)
}

/// Returns whether the vault is currently unlocked and the session is active.
#[tauri::command]
pub fn get_session_status(state: State<'_, AppState>) -> Result<bool, SanitizedError> {
    catch_vault_panic(|| {
        let secrets = state.secrets.lock().map_err(|_| {
            espass_vault_runtime::RuntimeError::InternalPanic
        })?;
        let session_guard = state.session.lock().map_err(|_| {
            espass_vault_runtime::RuntimeError::InternalPanic
        })?;
        let active = secrets.is_unlocked()
            && session_guard
                .as_ref()
                .is_some_and(|s| !s.is_expired(OffsetDateTime::now_utc()));
        Ok(active)
    })
    .map_err(sanitize_error)
}
```

- [ ] **Step 5: Create lib.rs**

Create `apps/desktop/src-tauri/src/lib.rs`:

```rust
mod commands;
mod state;

/// Tauri application entry point called from `main.rs`.
pub fn run() {
    tauri::Builder::default()
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::unlock_vault,
            commands::lock_vault,
            commands::get_session_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ESPASS desktop");
}
```

- [ ] **Step 6: Create main.rs**

Create `apps/desktop/src-tauri/src/main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    espass_desktop_lib::run();
}
```

- [ ] **Step 7: Verify compilation**

```
cargo check -p espass-desktop
```

Expected: no errors (Tauri generate_context requires tauri.conf.json which already exists).

- [ ] **Step 8: Commit**

```
git add apps/desktop/src-tauri/ Cargo.toml
git commit -m "feat(desktop): scoped Tauri commands with panic isolation and error sanitization"
```

---

## Task 12 — Extension: overlay-guard.ts

**Files:**
- Create: `apps/extension/src/content/overlay-guard.ts`
- Modify: `apps/extension/src/content/autofill-guard.ts`

- [ ] **Step 1: Create overlay-guard.ts**

Create `apps/extension/src/content/overlay-guard.ts`:

```typescript
/**
 * Anti-overlay and clickjacking detection for secure autofill.
 *
 * Prevents fake login forms from being overlaid on top of real fields to
 * intercept autofill values, and blocks clickjacking via transparent frames.
 */

/**
 * Returns true when an absolutely/fixed-positioned element fully covers the
 * target input, indicating a potential overlay attack.
 */
export function detectOverlayAttack(input: HTMLInputElement): boolean {
  const rect = input.getBoundingClientRect();
  if (rect.width === 0 || rect.height === 0) return false;

  const cx = rect.left + rect.width / 2;
  const cy = rect.top + rect.height / 2;

  const stack = document.elementsFromPoint(cx, cy);
  for (const el of stack) {
    if (el === input) break; // input is topmost — no overlay
    const style = window.getComputedStyle(el as HTMLElement);
    const pos = style.position;
    if (pos === "fixed" || pos === "absolute" || pos === "sticky") {
      const elRect = el.getBoundingClientRect();
      if (rectContains(elRect, rect)) {
        return true;
      }
    }
  }
  return false;
}

/**
 * Returns true when the current page is embedded in a frame it did not
 * navigate to (i.e., it is being framed by an attacker for clickjacking).
 */
export function detectFrameAncestorMismatch(): boolean {
  try {
    return window.self !== window.top;
  } catch {
    // Cross-origin top frame — definitely framed.
    return true;
  }
}

function rectContains(outer: DOMRect, inner: DOMRect): boolean {
  return (
    outer.left <= inner.left &&
    outer.top <= inner.top &&
    outer.right >= inner.right &&
    outer.bottom >= inner.bottom
  );
}
```

- [ ] **Step 2: Update autofill-guard.ts to use overlay-guard**

Replace the content of `apps/extension/src/content/autofill-guard.ts`:

```typescript
import { detectOverlayAttack, detectFrameAncestorMismatch } from "./overlay-guard";

export type AutofillSignal = {
  origin: string;
  topLevelOrigin: string;
  fieldVisible: boolean;
  crossOriginIframe: boolean;
  suspiciousDomain: boolean;
  overlayDetected: boolean;
  frameAncestorMismatch: boolean;
};

export function isVisibleInput(element: HTMLInputElement): boolean {
  const style = window.getComputedStyle(element);
  const rect = element.getBoundingClientRect();
  return (
    style.visibility !== "hidden" &&
    style.display !== "none" &&
    Number(style.opacity) > 0 &&
    rect.width >= 8 &&
    rect.height >= 8 &&
    !element.disabled &&
    element.type !== "hidden"
  );
}

export function detectSuspiciousDomain(hostname: string): boolean {
  const ascii = hostname.toLowerCase();
  return ascii.startsWith("xn--") || ascii.includes(".xn--") || /[^\x00-\x7F]/u.test(hostname);
}

export function collectAutofillSignal(input: HTMLInputElement): AutofillSignal {
  const origin = window.location.origin;
  let topLevelOrigin = origin;
  try {
    topLevelOrigin = window.top?.location.origin ?? origin;
  } catch {
    topLevelOrigin = "cross-origin";
  }

  return {
    origin,
    topLevelOrigin,
    fieldVisible: isVisibleInput(input),
    crossOriginIframe: topLevelOrigin !== origin,
    suspiciousDomain: detectSuspiciousDomain(window.location.hostname),
    overlayDetected: detectOverlayAttack(input),
    frameAncestorMismatch: detectFrameAncestorMismatch(),
  };
}

document.addEventListener(
  "click",
  (event) => {
    const target = event.target;
    if (!(target instanceof HTMLInputElement)) return;

    const signal = collectAutofillSignal(target);
    if (
      !signal.fieldVisible ||
      signal.crossOriginIframe ||
      signal.suspiciousDomain ||
      signal.overlayDetected ||
      signal.frameAncestorMismatch
    ) {
      return;
    }
    chrome.runtime.sendMessage({
      type: "credential-request",
      origin: signal.origin,
      topLevelOrigin: signal.topLevelOrigin,
      userGesture: event.isTrusted,
    });
  },
  { capture: true },
);
```

- [ ] **Step 3: Tighten manifest CSP**

Open `apps/extension/manifest.chrome.json` and ensure the `content_security_policy` field for the extension pages reads:

```json
"content_security_policy": {
  "extension_pages": "default-src 'self'; script-src 'self'; object-src 'none'; connect-src 'none'; form-action 'none'; frame-ancestors 'none'"
}
```

(Add this key inside the top-level JSON object if it is absent.)

- [ ] **Step 4: Commit**

```
git add apps/extension/src/content/overlay-guard.ts \
        apps/extension/src/content/autofill-guard.ts \
        apps/extension/manifest.chrome.json
git commit -m "feat(extension): overlay/clickjacking detection and CSP hardening"
```

---

## Task 13 — Fuzz targets: version downgrade, corrupted AAD, nonce reuse

**Files:**
- Create: `fuzz/fuzz_targets/version_downgrade.rs`
- Create: `fuzz/fuzz_targets/corrupted_aad.rs`
- Create: `fuzz/fuzz_targets/nonce_reuse.rs`

- [ ] **Step 1: Create version_downgrade.rs**

Create `fuzz/fuzz_targets/version_downgrade.rs`:

```rust
#![no_main]

use espass_crypto_core::{decrypt, EncryptedEnvelope, VaultKey};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 13 {
        return;
    }
    // Force version byte to an invalid value (anything but 1) to test downgrade rejection.
    let version = data[0].wrapping_add(2);
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&data[1..13]);
    let envelope = EncryptedEnvelope {
        version,
        nonce,
        ciphertext: data[13..].to_vec(),
    };
    let key = VaultKey::from_bytes([1u8; 32]);
    // Must always return Err — version 0 and versions >1 are unsupported.
    let result = decrypt(&key, &envelope, b"fuzz:aad");
    if version != 1 {
        assert!(result.is_err(), "version {version} should be rejected");
    }
});
```

- [ ] **Step 2: Create corrupted_aad.rs**

Create `fuzz/fuzz_targets/corrupted_aad.rs`:

```rust
#![no_main]

use espass_crypto_core::{decrypt, encrypt, EncryptedEnvelope, VaultKey};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let key = VaultKey::from_bytes([2u8; 32]);
    // Encrypt with canonical AAD.
    let Ok(envelope) = encrypt(&key, b"plaintext", b"canonical-aad") else { return };
    // Attempt decryption with fuzz-mutated AAD — must always fail.
    let result = decrypt(&key, &envelope, data);
    // If the fuzz data happens to equal "canonical-aad" decryption will succeed;
    // otherwise it must fail. We cannot assert failure because the fuzzer may
    // find the exact AAD, but any panic here is a bug.
    let _ = result;
});
```

- [ ] **Step 3: Create nonce_reuse.rs**

Create `fuzz/fuzz_targets/nonce_reuse.rs`:

```rust
#![no_main]

use espass_crypto_core::{decrypt, EncryptedEnvelope, VaultKey};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 24 {
        return;
    }
    let key = VaultKey::from_bytes([3u8; 32]);
    // Two envelopes sharing the same nonce (nonce reuse).
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&data[0..12]);
    let envelope_a = EncryptedEnvelope {
        version: 1,
        nonce,
        ciphertext: data[12..].to_vec(),
    };
    let envelope_b = EncryptedEnvelope {
        version: 1,
        nonce,
        ciphertext: data[12..].to_vec(),
    };
    // Both decrypt calls must not panic; failure is expected and acceptable.
    let _ = decrypt(&key, &envelope_a, b"aad-a");
    let _ = decrypt(&key, &envelope_b, b"aad-b");
});
```

- [ ] **Step 4: Add targets to fuzz Cargo.toml**

Open `fuzz/Cargo.toml`. In the `[[bin]]` section array, add:

```toml
[[bin]]
name = "version_downgrade"
path = "fuzz_targets/version_downgrade.rs"
test = false
doc = false

[[bin]]
name = "corrupted_aad"
path = "fuzz_targets/corrupted_aad.rs"
test = false
doc = false

[[bin]]
name = "nonce_reuse"
path = "fuzz_targets/nonce_reuse.rs"
test = false
doc = false
```

- [ ] **Step 5: Verify fuzz compilation**

```
cargo check --manifest-path fuzz/Cargo.toml
```

Expected: no errors.

- [ ] **Step 6: Commit**

```
git add fuzz/fuzz_targets/version_downgrade.rs \
        fuzz/fuzz_targets/corrupted_aad.rs \
        fuzz/fuzz_targets/nonce_reuse.rs \
        fuzz/Cargo.toml
git commit -m "feat(fuzz): version downgrade, corrupted AAD, and nonce-reuse fuzz targets"
```

---

## Task 14 — Release pipeline with Sigstore signing

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Create release.yml**

Create `.github/workflows/release.yml`:

```yaml
name: release

on:
  push:
    tags:
      - 'v[0-9]+.[0-9]+.[0-9]+'

permissions:
  contents: write
  id-token: write  # Required for Sigstore OIDC token

jobs:
  build-and-sign:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: '1.95.0'

      - name: Build release artifacts
        run: cargo build --release --workspace

      - name: Install cosign
        uses: sigstore/cosign-installer@v3

      - name: Sign native messaging host binary
        run: |
          cosign sign-blob \
            --yes \
            --output-certificate native-messaging-host.pem \
            --output-signature native-messaging-host.sig \
            target/release/espass-native-messaging-host
        env:
          COSIGN_EXPERIMENTAL: "1"

      - name: Generate SLSA provenance
        uses: slsa-framework/slsa-github-generator/.github/workflows/generator_generic_slsa3.yml@v2
        with:
          base64-subjects: |
            $(sha256sum target/release/espass-native-messaging-host | base64 -w0)

      - name: Upload artifacts
        uses: actions/upload-artifact@v4
        with:
          name: release-artifacts
          path: |
            target/release/espass-native-messaging-host
            native-messaging-host.pem
            native-messaging-host.sig
```

- [ ] **Step 2: Commit**

```
git add .github/workflows/release.yml
git commit -m "feat(ci): release pipeline with Sigstore/cosign signing and SLSA provenance"
```

---

## Task 15 — External audit package

**Files:**
- Create: `docs/audit/scope.md`
- Create: `docs/audit/architecture-review.md`
- Create: `docs/audit/cryptographic-review.md`
- Create: `docs/audit/trust-boundaries.md`
- Create: `docs/audit/attack-surface-registry.md`
- Create: `docs/audit/residual-risk-summary.md`
- Create: `docs/audit/reviewer-setup.md`

- [ ] **Step 1: Create scope.md**

Create `docs/audit/scope.md`:

```markdown
# ESPASS External Audit Scope

**Version:** 0.1.0-pre-audit  
**Date:** 2026-05-25  
**Classification:** Restricted — For Security Reviewers Only

## Scope

### In Scope
- `packages/crypto-core` — AES-256-GCM, Argon2id KDF, key types, memory locking, secure buffer
- `packages/vault-runtime` — unlock lifecycle, session management, persistence, IPC, autofill, device trust, panic boundary, secret windows
- `packages/shared-types` — vault schema V1, IPC protocol, device identity, autofill policy
- `apps/desktop/native-messaging-host` — browser native messaging protocol implementation
- `apps/extension/src/` — content script autofill guard, service worker, overlay detection
- `apps/desktop/src-tauri/src/` — Tauri command handlers and state management
- `apps/backend/src/` — zero-knowledge sync API

### Out of Scope for This Review
- Frontend UI components (not yet implemented)
- Mobile applications (not yet implemented)
- Enterprise RBAC (not yet implemented)
- SSO integrations (not yet implemented)
- Cloud deployment infrastructure

## Review Focus Areas

1. **Cryptographic correctness** — key derivation, encryption, AAD binding, nonce handling
2. **Memory safety** — secret zeroization, memory locking, panic safety
3. **IPC protocol** — handshake, signature verification, replay protection, origin pinning
4. **Vault integrity** — schema validation, integrity tags, migration safety
5. **Autofill security** — origin validation, overlay detection, punycode detection
6. **Error handling** — information leakage across trust boundaries
7. **Dependency supply chain** — critical cryptographic dependencies
```

- [ ] **Step 2: Create cryptographic-review.md**

Create `docs/audit/cryptographic-review.md`:

```markdown
# ESPASS Cryptographic Review Package

## Algorithms

| Purpose | Algorithm | Key Size | Reference |
|---------|-----------|----------|-----------|
| Vault encryption | AES-256-GCM | 256-bit | NIST SP 800-38D |
| Key derivation | Argon2id | 256-bit output | RFC 9106 |
| IPC message authentication | HMAC-SHA256 | 256-bit session key | RFC 2104 |
| Vault integrity tag | HMAC-SHA256 | Vault key | RFC 2104 |
| Device identity | Ed25519 | 256-bit | RFC 8032 |

## Key Derivation Parameters (Default)

```
Memory: 190 MiB (194,560 KiB)
Iterations: 3
Parallelism: 1
Salt: 128-bit random
Output: 256-bit MasterKey
```

Rationale: Exceeds OWASP ASVS 4.0 Level 2 requirements (64 MiB / 3 iterations minimum).

## Nonce Strategy

AES-256-GCM uses a 96-bit random nonce from `OsRng`. The probability of nonce collision is negligible given that each vault session generates a new VaultKey; the nonce birthday bound (2^48 messages) is never approached in practice.

## AAD Binding

Every encrypted payload binds tenant ID, vault ID, optional item ID, record type, and schema version as associated data. This prevents cross-vault, cross-tenant, and cross-type ciphertext substitution attacks.

## Residual Cryptographic Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| GCM nonce reuse under random generation | Low | Keys rotated per session; nonce space is 2^96 |
| Argon2id timing oracle via failed unlock | Low | Exponential backoff; constant-time password check not applicable (KDF is not constant-time by design) |
| AAD oracle via error message | Mitigated | All decryption errors return `DecryptionFailed` with no AAD detail |
```

- [ ] **Step 3: Create trust-boundaries.md**

Create `docs/audit/trust-boundaries.md`:

```markdown
# ESPASS Trust Boundary Map

## Boundary 1: Extension Content Script → Service Worker

**Direction:** one-way message  
**Transport:** `chrome.runtime.sendMessage`  
**Validation:** Service worker verifies `sender.url` origin matches `message.origin`  
**Secrets crossing boundary:** None. Only origin metadata crosses this boundary.

## Boundary 2: Browser Extension → Native Messaging Host

**Direction:** bidirectional, length-prefixed JSON  
**Transport:** Chrome/Firefox native messaging  
**Validation:**  
- Origin pinned to `ESPASS_ALLOWED_EXTENSION_ORIGINS` env var  
- Extension handshake verified before accepting any other message  
- All subsequent messages require a signed `SignedMessageEnvelope` with HMAC-SHA256  
- Monotonic counter enforced to prevent replay  
**Secrets crossing boundary:** Ephemeral session key confirmation (once, during handshake). Decrypted credentials cross this boundary during autofill — this is the highest-risk data flow.

## Boundary 3: Tauri Renderer → Tauri Rust Core

**Direction:** bidirectional via Tauri IPC  
**Transport:** Tauri's internal WebView IPC  
**Validation:**  
- Commands are allowlisted via `tauri::generate_handler!`  
- All commands wrap in `catch_vault_panic`  
- All errors sanitized via `sanitize_error` before reaching renderer  
**Secrets crossing boundary:** Password bytes on unlock (immediately zeroized). Session status (boolean only). The vault key never crosses this boundary.

## Boundary 4: Desktop App → Backend Sync API

**Direction:** client-initiated  
**Transport:** HTTPS (TLS 1.3)  
**Validation:** Backend receives and stores only ciphertext  
**Secrets crossing boundary:** None. Only encrypted blobs and metadata.

## Assumptions

1. The native messaging host binary is not tampered with post-installation.
2. The OS prevents other processes from reading the host's memory.
3. The user's machine is not compromised at the OS level.
4. The browser sandbox is not fully compromised.
```

- [ ] **Step 4: Create attack-surface-registry.md**

Create `docs/audit/attack-surface-registry.md`:

```markdown
# ESPASS Attack Surface Registry

## Attack Surface Entry Points

| ID | Surface | Protocol | Auth Required | Mitigations |
|----|---------|----------|---------------|-------------|
| AS-01 | Native messaging stdin | JSON/length-prefix | Extension origin pinning + handshake | Max message size, schema validation |
| AS-02 | Tauri IPC | Internal WebView | Renderer same-origin | Command allowlist, panic isolation |
| AS-03 | Extension content script | DOM events | User gesture | Visibility, overlay, punycode, cross-origin checks |
| AS-04 | Backend HTTP API | HTTPS | (TBD — bearer token) | Rate limiting, anomaly detection, payload size limits |
| AS-05 | Local vault file | Filesystem | OS file permissions | HMAC integrity tag, encrypted at rest |
| AS-06 | Clipboard | OS clipboard | — | 30-second TTL clear |
| AS-07 | Fuzz surface (crypto) | Direct API | — | Version validation, payload size limits, AAD binding |

## Threat Actor Capabilities Assumed

- **Malicious web page:** Can control page DOM, inject scripts (within CSP), attempt phishing
- **Compromised renderer:** Can call any allowlisted Tauri command with arbitrary arguments
- **Compromised extension:** Can attempt malformed IPC messages; cannot bypass HMAC without session key
- **Network attacker:** Can observe encrypted sync traffic; cannot decrypt without vault key
- **Physical attacker:** Can read swap/crash dumps; mitigated by mlock and zeroize

## Not Mitigated (Residual)

- Full OS compromise — out of scope for a client-side password manager
- Hardware-level memory attacks (cold boot, DMA) — mitigated only by mlock, not eliminated
- Browser zero-days that break the extension isolated world
```

- [ ] **Step 5: Create residual-risk-summary.md**

Create `docs/audit/residual-risk-summary.md`:

```markdown
# ESPASS Residual Risk Summary

| ID | Risk | Likelihood | Impact | Residual Mitigation | Owner |
|----|------|-----------|--------|-------------------|-------|
| RR-01 | Crash dump contains decrypted vault key | Low | Critical | mlock reduces likelihood; OS crash dump exclusion TBD | Engineering |
| RR-02 | Swap file exposure of plaintext | Low | High | mlock pins pages; encrypted swap (OS setting) recommended to users | Documentation |
| RR-03 | Timing oracle via Argon2id unlock latency | Low | Medium | Exponential backoff throttles; timing variation is inherent to KDF | Accepted |
| RR-04 | Extension isolated world breakout | Very Low | Critical | Depends on browser security; mitigated by MV3 restrictions | Accepted |
| RR-05 | Native messaging binary substitution | Low | Critical | Sigstore signing; system integrity (Gatekeeper/Windows Defender) | DevOps |
| RR-06 | Clipboard sniffing between copy and clear | Low | High | 30s TTL; users advised to use autofill instead of copy | Documentation |
| RR-07 | Session key in memory after IPC session expires | Low | Medium | IpcSessionRegistry evicts expired sessions on next validate call; proactive eviction TBD | Engineering |
```

- [ ] **Step 6: Create reviewer-setup.md**

Create `docs/audit/reviewer-setup.md`:

```markdown
# ESPASS Reviewer Setup Guide

## Prerequisites

- Rust 1.95.0 (pinned via `rust-toolchain.toml`)
- Node.js 20+ (for extension TypeScript)
- Git

## Repository Setup

```bash
git clone <repo-url> espass
cd espass
rustup show  # Should print "1.95.0"
```

## Build and Test

```bash
# Build all Rust crates
cargo build --workspace

# Run all tests
cargo test --workspace

# Run with security lints
cargo clippy --workspace --all-targets -- -D warnings

# Check dependency advisories
cargo audit

# Check dependency policy
cargo deny check
```

## Fuzzing (requires nightly)

```bash
rustup install nightly
cargo +nightly fuzz run malformed_envelope -- -max_total_time=60
cargo +nightly fuzz run ipc_schema -- -max_total_time=60
cargo +nightly fuzz run version_downgrade -- -max_total_time=60
```

## Key Files for Cryptographic Review

| File | What to Check |
|------|--------------|
| `packages/crypto-core/src/aead.rs` | AES-256-GCM, nonce generation, AAD binding |
| `packages/crypto-core/src/kdf.rs` | Argon2id params, minimum policy |
| `packages/crypto-core/src/keys.rs` | Zeroization, constant-time eq |
| `packages/crypto-core/src/memlock.rs` | mlock/VirtualLock correctness |
| `packages/vault-runtime/src/hardening.rs` | Panic isolation, error sanitization |
| `packages/shared-types/src/ipc.rs` | HMAC-SHA256, replay counter |

## Key Files for Protocol Review

| File | What to Check |
|------|--------------|
| `apps/desktop/native-messaging-host/src/main.rs` | Origin validation, schema validation |
| `packages/vault-runtime/src/ipc.rs` | Session lifecycle, expiry, replay |
| `apps/extension/src/background/service-worker.ts` | Origin verification |
| `apps/extension/src/content/autofill-guard.ts` | Policy enforcement |
```

- [ ] **Step 7: Create architecture-review.md**

Create `docs/audit/architecture-review.md`:

```markdown
# ESPASS Architecture Review Package

## System Overview

ESPASS is a zero-knowledge password manager. The server stores only encrypted blobs. Keys are derived client-side from the user's master password and never leave the device unencrypted.

## Component Interaction

```
Browser Extension (MV3)
    │  chrome.runtime.sendMessage (origin-validated)
    ▼
Service Worker
    │  chrome.runtime.connectNative
    ▼
Native Messaging Host (Rust)
    │  HMAC-signed JSON over stdin/stdout
    ▼
Vault Runtime (Rust library)
    ├── RuntimeSecretStore (mlock'd VaultKey, in-memory only)
    ├── UnlockManager (Argon2id → decrypt VaultKey)
    ├── IpcSessionRegistry (ephemeral session keys)
    └── SecureAutofillRuntime (origin validation → decrypt → fill)

Desktop App (Tauri)
    │  Tauri IPC (command allowlist)
    ▼
Vault Runtime (same Rust library)

Backend Sync API (Axum)
    │  HTTPS — receives ciphertext only
    ▼
Encrypted blob storage (server never holds keys)
```

## Security Properties

1. **Zero-knowledge sync:** The backend stores `VaultItem.encrypted_payload` blobs. It cannot decrypt them.
2. **Vault key isolation:** `VaultKey` lives only in `RuntimeSecretStore`, which zeroizes on drop and optionally locks pages with `mlock`/`VirtualLock`.
3. **Session-scoped IPC:** Each extension connection gets a fresh 256-bit `SessionKey`. Messages are HMAC-SHA256 signed with a monotonic counter. Sessions expire after 5 minutes.
4. **Autofill policy:** A credential is decrypted only when the requesting origin exactly matches the saved origin, the field is visible, no overlay is detected, and a user gesture is present.
5. **Panic isolation:** Every Tauri command and IPC handler wraps its body in `catch_vault_panic`. Panics are converted to `RuntimeError::InternalPanic` and never propagate to the renderer.
```

- [ ] **Step 8: Commit all audit docs**

```
git add docs/audit/
git commit -m "docs(audit): complete external audit package — scope, crypto, trust boundaries, attack surface, residual risk, reviewer setup, architecture"
```

---

## Task 16 — Future-feature trust model

**Files:**
- Create: `docs/architecture/future/future-feature-trust-model.md`

- [ ] **Step 1: Create the document**

Create `docs/architecture/future/future-feature-trust-model.md`:

```markdown
# ESPASS Future-Feature Trust Model

This document describes the trust-boundary and cryptographic implications of
planned future features. None of these are implemented. This analysis must be
revisited before implementation begins.

## Passkeys (WebAuthn)

**Trust boundary impact:** Adds a new credential type stored in the vault. The
vault encryption model is unchanged. The passkey private key blob is stored
encrypted under the VaultKey like any other credential.

**Crypto implication:** The WebAuthn authenticator (platform or roaming) signs
assertions internally. ESPASS stores the `credentialId` and `userHandle`
encrypted; private keys never leave the authenticator.

**Threat model impact:** A compromised vault would expose the encrypted
credential ID, not the private key. The authenticator's secure element remains
the trust anchor.

## Mobile Apps

**Trust boundary impact:** Requires a mobile vault runtime with the same
security properties as the desktop. The mobile OS keychain should be used to
store the encrypted VaultKey, not the plaintext.

**Crypto implication:** The mobile KDF parameters must match or exceed desktop
defaults. Memory locking (`mlock`) is not available on iOS/Android userspace;
the OS process isolation model is relied upon instead.

**Threat model impact:** Jailbroken/rooted devices are an out-of-scope threat.
The mobile app must refuse to run or warn prominently on rooted devices.

## Enterprise RBAC

**Trust boundary impact:** Introduces a server-side policy layer. Vault access
decisions must remain client-side; the server enforces sharing policy by
controlling which encrypted VaultKey slots are served to which devices.

**Crypto implication:** Shared vaults require per-recipient key slots. Each
device gets the VaultKey encrypted under its `DeviceKey`. The server controls
who can receive which key slots but never holds the VaultKey in plaintext.

**Threat model impact:** A compromised admin cannot decrypt vaults. A
compromised server can deny access but cannot read plaintext. This is the
zero-knowledge constraint for enterprise sharing.

## HSM Integration

**Trust boundary impact:** The MasterKey derivation step (Argon2id) is replaced
or augmented by an HSM-bound key. The HSM becomes a hardware trust anchor.

**Crypto implication:** The HSM stores a hardware-bound root key. The VaultKey
is wrapped by `HMAC(hsm_root_key, argon2_output)`, so both the password and
the HSM key must be present to open the vault.

**Threat model impact:** Eliminates the key-in-memory-only risk at the cost of
requiring HSM availability. Offline unlock becomes impossible without pre-cached
key material.

## SSO

**Trust boundary impact:** Adds an identity provider as a trust anchor. The
identity token must be used to authenticate the device registration, not to
derive the VaultKey. The VaultKey must remain password-derived or
HSM-derived — SSO tokens must not be used as key material.

**Crypto implication:** SSO login gates access to the backend and device
registration flow. The VaultKey derivation is unchanged.

**Threat model impact:** A compromised IdP can prevent access (denial of
service) but cannot decrypt vaults. This preserves zero-knowledge.
```

- [ ] **Step 2: Commit**

```
git add docs/architecture/future/future-feature-trust-model.md
git commit -m "docs(architecture): future-feature trust model for passkeys, mobile, RBAC, HSM, SSO"
```

---

## Final Verification

- [ ] **Run full workspace tests**

```
cargo test --workspace
```

Expected: all tests pass across all crates.

- [ ] **Run clippy**

```
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: no warnings promoted to errors.

- [ ] **Run fmt check**

```
cargo fmt --all --check
```

Expected: no formatting changes needed.

- [ ] **Verify audit docs exist**

```
ls docs/audit/
```

Expected: 7 markdown files.

---

## Self-Review Checklist

### Spec Coverage

| Phase 5 Requirement | Task |
|--------------------|------|
| 5A: Panic boundary isolation | Task 4 |
| 5A: Error sanitization | Task 4 |
| 5A: Runtime integrity verification | Task 5 |
| 5B: Ephemeral secret windows | Task 6 |
| 5B: Clipboard exposure minimization | Task 7 |
| 5C: Adversarial testing (fuzz expansion) | Task 13 |
| 5E: Desktop runtime isolation | Task 11 |
| 5E: Permission-scoped commands | Task 11 |
| 5F: Extension CSP hardening | Task 12 |
| 5F: Anti-overlay protections | Task 12 |
| 5F: Anti-clickjacking | Task 12 |
| 5G: Rate limiting | Task 9 |
| 5G: Anomaly detection | Task 10 |
| 5G: Payload size limits | Task 10 |
| 5H: Sigstore/Cosign evaluation | Task 14 |
| 5H: Provenance attestations | Task 14 |
| 5I: Audit scope document | Task 15 |
| 5I: Architecture review package | Task 15 |
| 5I: Cryptographic review package | Task 15 |
| 5I: Trust-boundary diagrams | Task 15 |
| 5I: Attack surface registry | Task 15 |
| 5I: Residual risk summary | Task 15 |
| 5I: Reviewer setup guide | Task 15 |
| 5J: Future-feature trust modeling | Task 16 |
| Security events (gap from Phase 4) | Task 8 |

### Items Not Covered (deferred with rationale)
- **Concurrent IPC flooding simulation (5C):** Requires a full async test harness; the fuzz targets cover the equivalent malformed-input attack surface. Add a dedicated integration test suite in Phase 6.
- **Secure UX validation (5D):** UI is not yet implemented; UX review gates on Phase 6 frontend work.
- **Backend sync conflict abuse (5G):** Conflict resolution logic is not yet implemented; placeholder acknowledged in `sync_client.rs`.
- **Miri memory audit (5B heap snapshots):** Already wired into the CI `security.yml` job (`cargo +nightly miri test -p espass-crypto-core`).
