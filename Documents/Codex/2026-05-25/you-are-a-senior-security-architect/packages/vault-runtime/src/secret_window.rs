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
