//! Privacy-safe security event schemas.
//!
//! These events intentionally avoid passwords, credential values, vault item
//! names, browsing history, URLs beyond coarse origin hashes supplied by
//! callers, and decrypted vault content.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// Privacy-safe security event.
///
/// Audit event type that records security-relevant occurrences without leaking
/// sensitive information like passwords, origins, or attempt counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEvent {
    /// Event kind in kebab-case (e.g., "vault-unlocked", "unlock-failed").
    pub kind: String,
    /// Optional device identifier.
    pub device_id: Option<Uuid>,
    /// Optional vault identifier.
    pub vault_id: Option<Uuid>,
    /// Timestamp when the event occurred.
    pub occurred_at: OffsetDateTime,
}

impl SecurityEvent {
    /// Creates a vault-unlocked event.
    ///
    /// Records that a vault was successfully unlocked by a device.
    pub fn vault_unlocked(device_id: Uuid, vault_id: Uuid) -> Self {
        Self {
            kind: "vault-unlocked".to_string(),
            device_id: Some(device_id),
            vault_id: Some(vault_id),
            occurred_at: OffsetDateTime::now_utc(),
        }
    }

    /// Creates an IPC handshake denied event.
    ///
    /// Records that an IPC handshake was rejected. The origin parameter is not
    /// stored to maintain privacy (no PII logged).
    pub fn ipc_handshake_denied(_origin: &str) -> Self {
        Self {
            kind: "ipc-handshake-denied".to_string(),
            device_id: None,
            vault_id: None,
            occurred_at: OffsetDateTime::now_utc(),
        }
    }

    /// Creates an unlock-failed event.
    ///
    /// Records that an unlock attempt failed. Attempt counts are not stored in
    /// the event to prevent rate-limit enumeration attacks.
    pub fn unlock_failed(device_id: Uuid) -> Self {
        Self {
            kind: "unlock-failed".to_string(),
            device_id: Some(device_id),
            vault_id: None,
            occurred_at: OffsetDateTime::now_utc(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_unlocked_event_serializes() {
        let device_id = Uuid::new_v4();
        let vault_id = Uuid::new_v4();
        let event = SecurityEvent::vault_unlocked(device_id, vault_id);

        let json = serde_json::to_string(&event).expect("serialization failed");
        assert!(json.contains("vault-unlocked"), "JSON should contain vault-unlocked");
        assert!(
            !json.contains("password"),
            "JSON should not contain password"
        );
    }

    #[test]
    fn ipc_handshake_denied_event_serializes() {
        let event = SecurityEvent::ipc_handshake_denied("https://example.com");
        let json = serde_json::to_string(&event).expect("serialization failed");
        assert!(
            json.contains("ipc-handshake-denied"),
            "JSON should contain ipc-handshake-denied"
        );
    }

    #[test]
    fn unlock_failed_event_does_not_leak_attempt_count_in_kind() {
        let device_id = Uuid::new_v4();
        let event = SecurityEvent::unlock_failed(device_id);
        let json = serde_json::to_string(&event).expect("serialization failed");
        assert!(json.contains("unlock-failed"), "JSON should contain unlock-failed");
        assert!(
            !json.contains("attempts"),
            "JSON should not contain attempts"
        );
    }
}
