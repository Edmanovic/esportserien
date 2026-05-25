//! Minimal encrypted sync client contract.

use espass_shared_types::vault::{EncryptedPayload, VaultItem};
use uuid::Uuid;

use crate::RuntimeError;

/// Sync client abstraction. Implementations move ciphertext only.
pub trait EncryptedSyncClient {
    /// Uploads an encrypted vault item.
    fn upload_item(&mut self, item: VaultItem) -> Result<(), RuntimeError>;
    /// Downloads an encrypted vault item.
    fn download_item(
        &self,
        vault_id: Uuid,
        item_id: Uuid,
    ) -> Result<Option<EncryptedPayload>, RuntimeError>;
}
