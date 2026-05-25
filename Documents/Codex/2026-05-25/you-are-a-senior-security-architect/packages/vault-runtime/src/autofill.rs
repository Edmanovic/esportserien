//! Secure autofill runtime.

use espass_crypto_core::{decrypt, EncryptedEnvelope, EncryptionMetadata, SecureBuffer};
use espass_shared_types::autofill::{
    AutofillDecision, AutofillPolicyEngine, OriginValidationRequest,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{RuntimeError, RuntimeSecretStore};

/// Minimal decrypted credential scope for one autofill operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialScope {
    /// Username or account identifier.
    pub username: String,
    /// Password in memory only. The extension receives it for immediate fill.
    pub password: String,
    /// Origin this credential is valid for.
    pub origin: String,
}

/// Broker validates request policy before any decryption happens.
#[derive(Debug, Clone, Copy, Default)]
pub struct CredentialRequestBroker;

impl CredentialRequestBroker {
    /// Validates a credential request.
    pub fn authorize(request: &OriginValidationRequest) -> Result<(), RuntimeError> {
        match AutofillPolicyEngine::evaluate(request) {
            AutofillDecision::Allow => Ok(()),
            AutofillDecision::Block | AutofillDecision::RequireUserGesture => {
                Err(RuntimeError::AutofillDenied)
            }
        }
    }
}

/// Runtime decryptor for credential blobs.
#[derive(Debug, Clone, Copy, Default)]
pub struct SecureAutofillRuntime;

impl SecureAutofillRuntime {
    /// Decrypts a single credential payload after origin verification.
    pub fn decrypt_for_fill(
        secrets: &RuntimeSecretStore,
        vault_id: Uuid,
        item_id: Uuid,
        request: &OriginValidationRequest,
        encrypted: &EncryptedEnvelope,
    ) -> Result<CredentialScope, RuntimeError> {
        CredentialRequestBroker::authorize(request)?;
        let key = secrets.vault_key()?;
        let metadata = EncryptionMetadata {
            tenant_id: "local".to_owned(),
            vault_id: vault_id.to_string(),
            item_id: Some(item_id.to_string()),
            record_type: "credential".to_owned(),
            schema_version: 1,
        };
        let plaintext = decrypt(key, encrypted, &metadata.to_aad())?;
        decode_credential_scope(&plaintext)
    }
}

fn decode_credential_scope(buffer: &SecureBuffer) -> Result<CredentialScope, RuntimeError> {
    serde_json::from_slice(buffer.expose_secret()).map_err(|_| RuntimeError::AutofillDenied)
}
