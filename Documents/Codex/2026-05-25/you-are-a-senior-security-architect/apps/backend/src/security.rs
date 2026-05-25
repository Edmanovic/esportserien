//! Backend security controls for encrypted-only sync.

use std::collections::BTreeMap;
use std::net::IpAddr;

use espass_shared_types::vault::EncryptedPayload;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

/// Fixed-window rate limiter for prototype API hardening.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    max_requests: u32,
    window_seconds: i64,
    buckets: BTreeMap<IpAddr, RateBucket>,
}

#[derive(Debug, Clone)]
struct RateBucket {
    count: u32,
    window_started_at: OffsetDateTime,
}

impl RateLimiter {
    /// Creates a limiter.
    #[must_use]
    pub fn new(max_requests: u32, window_seconds: i64) -> Self {
        Self {
            max_requests,
            window_seconds,
            buckets: BTreeMap::new(),
        }
    }

    /// Checks and records one request.
    pub fn check(&mut self, ip: IpAddr, now: OffsetDateTime) -> Result<(), BackendSecurityError> {
        let bucket = self.buckets.entry(ip).or_insert(RateBucket {
            count: 0,
            window_started_at: now,
        });
        if now - bucket.window_started_at >= Duration::seconds(self.window_seconds) {
            bucket.count = 0;
            bucket.window_started_at = now;
        }
        if bucket.count >= self.max_requests {
            return Err(BackendSecurityError::RateLimited);
        }
        bucket.count = bucket.count.saturating_add(1);
        Ok(())
    }
}

/// Replay protector for device-bound signed requests.
#[derive(Debug, Default, Clone)]
pub struct RequestReplayProtector {
    last_counter_by_device: BTreeMap<Uuid, u64>,
}

impl RequestReplayProtector {
    /// Validates that a device request counter advances monotonically.
    pub fn validate(&mut self, device_id: Uuid, counter: u64) -> Result<(), BackendSecurityError> {
        let last = self.last_counter_by_device.entry(device_id).or_insert(0);
        if counter <= *last {
            return Err(BackendSecurityError::Replay);
        }
        *last = counter;
        Ok(())
    }
}

/// Upload integrity verifier for encrypted payload boundaries.
pub struct UploadIntegrityVerifier;

impl UploadIntegrityVerifier {
    /// Validates encrypted payload shape without inspecting plaintext.
    pub fn validate(payload: &EncryptedPayload) -> Result<(), BackendSecurityError> {
        if payload.envelope_version != 1 {
            return Err(BackendSecurityError::InvalidPayload);
        }
        if payload.nonce == [0_u8; 12] {
            return Err(BackendSecurityError::InvalidPayload);
        }
        if payload.ciphertext.len() < 16 || payload.ciphertext.len() > 64 * 1024 * 1024 {
            return Err(BackendSecurityError::InvalidPayload);
        }
        if payload.aad_context.is_empty() || payload.aad_context.len() > 2048 {
            return Err(BackendSecurityError::InvalidPayload);
        }
        Ok(())
    }
}

/// Backend security failures.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BackendSecurityError {
    /// Request exceeded rate limits.
    #[error("rate limited")]
    RateLimited,
    /// Replay counter did not advance.
    #[error("replay detected")]
    Replay,
    /// Encrypted payload was malformed.
    #[error("invalid encrypted payload")]
    InvalidPayload,
}
