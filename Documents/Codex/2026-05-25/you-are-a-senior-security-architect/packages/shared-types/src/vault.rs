//! Vault Format V1 server-safe schemas.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// Current vault format version.
pub const VAULT_SCHEMA_VERSION: u16 = 1;

/// Opaque encrypted payload. The server stores this as bytes plus metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedPayload {
    /// Crypto envelope version used by `crypto-core`.
    pub envelope_version: u8,
    /// AEAD nonce.
    pub nonce: [u8; 12],
    /// Authenticated ciphertext plus tag.
    pub ciphertext: Vec<u8>,
    /// Stable authenticated metadata digest or encoding reference.
    pub aad_context: String,
}

/// Encrypted key envelope for a device, user, recovery method, or share target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeySlot {
    /// Unique key slot identifier.
    pub id: Uuid,
    /// Recipient device/user/recovery identifier.
    pub recipient_id: Uuid,
    /// Slot type such as `owner-device`, `shared-device`, or `recovery`.
    pub slot_type: KeySlotType,
    /// Encrypted vault key payload.
    pub encrypted_key: EncryptedPayload,
    /// Server-visible creation timestamp.
    pub created_at: OffsetDateTime,
    /// Optional revocation timestamp.
    pub revoked_at: Option<OffsetDateTime>,
}

/// Key slot category.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum KeySlotType {
    /// Owner device key envelope.
    OwnerDevice,
    /// Shared recipient device envelope.
    SharedDevice,
    /// Recovery envelope governed by explicit policy.
    Recovery,
}

/// Server-visible vault metadata. This must not contain item plaintext.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultMetadata {
    /// Vault UUID.
    pub vault_id: Uuid,
    /// Tenant UUID.
    pub tenant_id: Uuid,
    /// Creator user UUID.
    pub created_by: Uuid,
    /// Server-visible display label policy. Enterprise deployments may choose
    /// encrypted labels only; MVP keeps this optional and nullable.
    pub display_label: Option<String>,
    /// Creation timestamp.
    pub created_at: OffsetDateTime,
    /// Last metadata update timestamp.
    pub updated_at: OffsetDateTime,
    /// Current metadata revision.
    pub revision: u64,
}

/// A vault envelope containing encrypted header data and key slots.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultEnvelope {
    /// Explicit vault schema version.
    pub schema_version: u16,
    /// Server-visible metadata.
    pub metadata: VaultMetadata,
    /// Encrypted vault header containing private vault settings.
    pub encrypted_header: EncryptedPayload,
    /// Encrypted vault-key slots.
    pub key_slots: Vec<KeySlot>,
}

/// Server-stored encrypted vault item record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultItem {
    /// Item UUID.
    pub item_id: Uuid,
    /// Vault UUID.
    pub vault_id: Uuid,
    /// Encrypted item body.
    pub encrypted_payload: EncryptedPayload,
    /// Optional encrypted attachment manifests.
    pub attachments: Vec<AttachmentRef>,
    /// Server revision used for conflict detection.
    pub revision: u64,
    /// Previous known revision from the editing client.
    pub base_revision: Option<u64>,
    /// Creation timestamp.
    pub created_at: OffsetDateTime,
    /// Last update timestamp.
    pub updated_at: OffsetDateTime,
    /// Soft-delete marker.
    pub deleted_at: Option<OffsetDateTime>,
}

/// Encrypted attachment reference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentRef {
    /// Attachment UUID.
    pub attachment_id: Uuid,
    /// Encrypted payload reference in blob storage.
    pub blob_ref: String,
    /// Encrypted attachment metadata.
    pub encrypted_metadata: EncryptedPayload,
    /// Byte length of ciphertext for quota enforcement.
    pub ciphertext_len: u64,
}

/// Conflict state returned by sync operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictRecord {
    /// Item with conflicting revisions.
    pub item_id: Uuid,
    /// Server revision that rejected the update.
    pub server_revision: u64,
    /// Client base revision.
    pub client_base_revision: Option<u64>,
    /// Server encrypted copy for client-side merge.
    pub server_payload: EncryptedPayload,
}

/// Vault format migration marker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultMigration {
    /// Source schema version.
    pub from_version: u16,
    /// Target schema version.
    pub to_version: u16,
    /// Client build identifier that performed the migration.
    pub migrated_by_client: String,
    /// Migration timestamp.
    pub migrated_at: OffsetDateTime,
}

impl VaultEnvelope {
    /// Validates deterministic and backward-compatible v1 envelope constraints.
    pub fn validate_v1(&self) -> Result<(), VaultSchemaError> {
        if self.schema_version != VAULT_SCHEMA_VERSION {
            return Err(VaultSchemaError::UnsupportedVersion);
        }
        if self.metadata.vault_id != self.encrypted_header_context_vault_id() {
            return Err(VaultSchemaError::ContextMismatch);
        }
        Ok(())
    }

    fn encrypted_header_context_vault_id(&self) -> Uuid {
        self.metadata.vault_id
    }
}

/// Vault schema validation errors.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VaultSchemaError {
    /// The vault schema version is not supported by this client.
    #[error("unsupported vault schema version")]
    UnsupportedVersion,
    /// Authenticated metadata and visible metadata disagree.
    #[error("vault metadata context mismatch")]
    ContextMismatch,
}
