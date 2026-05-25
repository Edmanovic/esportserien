//! Protocol boundary tests for shared ESPASS schemas.

use espass_shared_types::autofill::{
    AutofillDecision, AutofillPolicyEngine, OriginValidationRequest,
};
use espass_shared_types::ipc::{IpcPayload, SignedMessageEnvelope};
use espass_shared_types::session::{AccessSession, SessionError, SessionPolicy};
use time::OffsetDateTime;
use uuid::Uuid;

#[test]
fn autofill_blocks_cross_origin_iframe() {
    let request = OriginValidationRequest {
        origin: "https://login.example.com".to_owned(),
        top_level_origin: "https://evil.example".to_owned(),
        saved_origin: "https://login.example.com".to_owned(),
        field_visible: true,
        cross_origin_iframe: true,
        user_gesture: true,
        suspicious_domain: false,
    };

    assert_eq!(
        AutofillPolicyEngine::evaluate(&request),
        AutofillDecision::Block
    );
}

#[test]
fn ipc_signature_detects_tampering() -> Result<(), Box<dyn std::error::Error>> {
    let session_key = [3_u8; 32];
    let payload = IpcPayload::PermissionDecision {
        origin: "https://example.com".to_owned(),
        granted: true,
    };
    let mut envelope = SignedMessageEnvelope::sign(
        Uuid::new_v4(),
        Uuid::new_v4(),
        1,
        OffsetDateTime::UNIX_EPOCH,
        payload,
        &session_key,
    )?;

    envelope.counter = 2;
    assert!(envelope.verify(&session_key).is_err());
    Ok(())
}

#[test]
fn session_rejects_replay_counter() {
    let user_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let now = OffsetDateTime::UNIX_EPOCH;
    let mut session = AccessSession::new(user_id, device_id, now, SessionPolicy::default());

    assert_eq!(session.validate_request(device_id, now, 1), Ok(()));
    assert_eq!(
        session.validate_request(device_id, now, 1),
        Err(SessionError::ReplayDetected)
    );
}
