//! Device-bound session and refresh-token schemas.

use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Short-lived device-bound access session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessSession {
    /// Session UUID.
    pub session_id: Uuid,
    /// User UUID.
    pub user_id: Uuid,
    /// Bound device UUID.
    pub device_id: Uuid,
    /// Issued timestamp.
    pub issued_at: OffsetDateTime,
    /// Idle expiry timestamp.
    pub idle_expires_at: OffsetDateTime,
    /// Absolute expiry timestamp.
    pub absolute_expires_at: OffsetDateTime,
    /// Replay counter. Requests must advance monotonically.
    pub replay_counter: u64,
    /// Revocation marker.
    pub revoked_at: Option<OffsetDateTime>,
}

/// Refresh token record. Store only the hash server-side.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RefreshTokenRecord {
    /// Token family UUID for rotation chains.
    pub family_id: Uuid,
    /// Current token UUID.
    pub token_id: Uuid,
    /// User UUID.
    pub user_id: Uuid,
    /// Device UUID.
    pub device_id: Uuid,
    /// SHA-256 hash of random token bytes plus server pepper context.
    pub token_hash: [u8; 32],
    /// Issued timestamp.
    pub issued_at: OffsetDateTime,
    /// Expiry timestamp.
    pub expires_at: OffsetDateTime,
    /// Rotation timestamp.
    pub rotated_at: Option<OffsetDateTime>,
    /// Revocation timestamp.
    pub revoked_at: Option<OffsetDateTime>,
}

/// Session policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionPolicy {
    /// Access token lifetime.
    pub access_ttl_seconds: i64,
    /// Idle timeout.
    pub idle_timeout_seconds: i64,
    /// Absolute session timeout.
    pub absolute_timeout_seconds: i64,
}

impl Default for SessionPolicy {
    fn default() -> Self {
        Self {
            access_ttl_seconds: 900,
            idle_timeout_seconds: 900,
            absolute_timeout_seconds: 43_200,
        }
    }
}

impl AccessSession {
    /// Creates a new device-bound session.
    #[must_use]
    pub fn new(user_id: Uuid, device_id: Uuid, now: OffsetDateTime, policy: SessionPolicy) -> Self {
        Self {
            session_id: Uuid::new_v4(),
            user_id,
            device_id,
            issued_at: now,
            idle_expires_at: now + Duration::seconds(policy.idle_timeout_seconds),
            absolute_expires_at: now + Duration::seconds(policy.absolute_timeout_seconds),
            replay_counter: 0,
            revoked_at: None,
        }
    }

    /// Validates expiry and replay counter advancement.
    pub fn validate_request(
        &mut self,
        device_id: Uuid,
        now: OffsetDateTime,
        request_counter: u64,
    ) -> Result<(), SessionError> {
        if self.revoked_at.is_some() {
            return Err(SessionError::Revoked);
        }
        if self.device_id != device_id {
            return Err(SessionError::DeviceMismatch);
        }
        if now >= self.idle_expires_at || now >= self.absolute_expires_at {
            return Err(SessionError::Expired);
        }
        if request_counter <= self.replay_counter {
            return Err(SessionError::ReplayDetected);
        }
        self.replay_counter = request_counter;
        Ok(())
    }
}

/// Generates a random refresh token and its SHA-256 hash.
pub fn generate_refresh_token() -> ([u8; 32], [u8; 32]) {
    let mut token = [0_u8; 32];
    OsRng.fill_bytes(&mut token);
    let hash = Sha256::digest(token);
    (token, hash.into())
}

/// Computes a keyed request proof for anti-token-theft request binding.
pub fn request_hmac(
    token: &[u8],
    method: &str,
    path: &str,
    body_digest: &[u8],
) -> Result<[u8; 32], SessionError> {
    let mut mac = HmacSha256::new_from_slice(token).map_err(|_| SessionError::InvalidToken)?;
    mac.update(b"espass:request-proof:v1\n");
    mac.update(method.as_bytes());
    mac.update(b"\n");
    mac.update(path.as_bytes());
    mac.update(b"\n");
    mac.update(body_digest);
    Ok(mac.finalize().into_bytes().into())
}

/// Session errors.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SessionError {
    /// Session expired.
    #[error("session expired")]
    Expired,
    /// Session revoked.
    #[error("session revoked")]
    Revoked,
    /// Request came from the wrong device.
    #[error("session device mismatch")]
    DeviceMismatch,
    /// Replay counter did not advance.
    #[error("session replay detected")]
    ReplayDetected,
    /// Token was invalid.
    #[error("invalid token")]
    InvalidToken,
}
