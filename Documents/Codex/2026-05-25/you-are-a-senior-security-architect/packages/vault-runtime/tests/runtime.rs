//! Secure runtime integration tests.

use espass_crypto_core::{encrypt, EncryptionMetadata, VaultKey};
use espass_shared_types::autofill::OriginValidationRequest;
use espass_shared_types::ipc::{
    ExtensionHandshake, ExtensionPlatform, IpcPayload, IpcPermissionModel, SignedMessageEnvelope,
};
use espass_vault_runtime::{
    CredentialScope, ExtensionTrustValidator, IpcHandshakeManager, IpcSessionRegistry,
    RuntimeError, RuntimeSecretStore, SecureAutofillRuntime, SessionRuntime,
    VaultPersistenceEngine, VaultStore,
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[test]
fn local_vault_persistence_detects_tampering() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("vault.espass");
    let key = VaultKey::from_bytes([4_u8; 32]);
    let store = VaultStore::new(path);
    let engine = VaultPersistenceEngine::new(store.clone());

    let record = engine.persist(&key, Uuid::new_v4(), br#"{"items":[]}"#, None)?;
    let mut tampered = record;
    tampered.local_revision = tampered.local_revision.saturating_add(1);
    store.save(&tampered)?;

    assert_eq!(store.load(&key).err(), Some(RuntimeError::Integrity));
    Ok(())
}

#[test]
fn ipc_registry_rejects_replay() -> Result<(), Box<dyn std::error::Error>> {
    let permission_model = IpcPermissionModel {
        allowed_extension_origins: vec!["chrome-extension://abc/".to_owned()],
        max_message_bytes: 1024,
        require_user_gesture_for_autofill: true,
    };
    let validator = ExtensionTrustValidator::new(permission_model, 1);
    let manager = IpcHandshakeManager::new(validator);
    let handshake = ExtensionHandshake {
        extension_origin: "chrome-extension://abc/".to_owned(),
        platform: ExtensionPlatform::ChromiumMv3,
        extension_version: "0.1.0".to_owned(),
        extension_nonce: [1_u8; 32],
        protocol_version: 1,
    };
    let now = OffsetDateTime::UNIX_EPOCH;
    let (accepted, key) = manager.accept(&handshake, now)?;
    let mut registry = IpcSessionRegistry::new();
    registry.insert(accepted.session.clone(), key);

    let payload = IpcPayload::PermissionDecision {
        origin: "https://example.com".to_owned(),
        granted: true,
    };
    let request = SignedMessageEnvelope::sign(
        accepted.session.session_id,
        Uuid::new_v4(),
        1,
        now,
        payload,
        &accepted.session_key_confirmation,
    )?;

    assert_eq!(registry.validate(&request, now), Ok(()));
    assert_eq!(registry.validate(&request, now), Err(RuntimeError::Ipc));
    Ok(())
}

#[test]
fn autofill_decrypts_only_after_origin_policy_allows() -> Result<(), Box<dyn std::error::Error>> {
    let vault_id = Uuid::new_v4();
    let item_id = Uuid::new_v4();
    let key = VaultKey::from_bytes([5_u8; 32]);
    let mut secrets = RuntimeSecretStore::locked();
    secrets.open(vault_id, key)?;

    let metadata = EncryptionMetadata {
        tenant_id: "local".to_owned(),
        vault_id: vault_id.to_string(),
        item_id: Some(item_id.to_string()),
        record_type: "credential".to_owned(),
        schema_version: 1,
    };
    let credential = CredentialScope {
        username: "alice".to_owned(),
        password: "correct horse".to_owned(),
        origin: "https://example.com".to_owned(),
    };
    let encrypted = encrypt(
        secrets.vault_key()?,
        &serde_json::to_vec(&credential)?,
        &metadata.to_aad(),
    )?;
    let request = OriginValidationRequest {
        origin: "https://example.com".to_owned(),
        top_level_origin: "https://example.com".to_owned(),
        saved_origin: "https://example.com".to_owned(),
        field_visible: true,
        cross_origin_iframe: false,
        user_gesture: true,
        suspicious_domain: false,
    };

    let decrypted =
        SecureAutofillRuntime::decrypt_for_fill(&secrets, vault_id, item_id, &request, &encrypted)?;
    assert_eq!(decrypted.username, "alice");

    let blocked_request = OriginValidationRequest {
        cross_origin_iframe: true,
        top_level_origin: "https://evil.example".to_owned(),
        ..request
    };
    assert_eq!(
        SecureAutofillRuntime::decrypt_for_fill(
            &secrets,
            vault_id,
            item_id,
            &blocked_request,
            &encrypted
        )
        .err(),
        Some(RuntimeError::AutofillDenied)
    );
    Ok(())
}

#[test]
fn session_runtime_expires_and_locks() -> Result<(), Box<dyn std::error::Error>> {
    let mut secrets = RuntimeSecretStore::locked();
    secrets.open(Uuid::new_v4(), VaultKey::from_bytes([6_u8; 32]))?;
    let session = SessionRuntime::new(OffsetDateTime::UNIX_EPOCH, 1, 10);
    let engine = espass_vault_runtime::AutoLockEngine::default();
    let result = engine.enforce(
        &session,
        &mut secrets,
        OffsetDateTime::UNIX_EPOCH + Duration::seconds(2),
    );
    assert_eq!(result, Err(RuntimeError::Session));
    assert!(!secrets.is_unlocked());
    Ok(())
}
