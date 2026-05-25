//! Device identity, registration, and trust schemas.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// Device trust state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceTrustState {
    /// Registration is pending verification.
    Pending,
    /// Device is trusted for the account or tenant.
    Trusted,
    /// Device has been revoked and cannot sync.
    Revoked,
    /// Device key rotation is in progress.
    Rotating,
}

/// Public device identity stored by backend and clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceIdentity {
    /// Device UUID.
    pub device_id: Uuid,
    /// Owning user UUID.
    pub user_id: Uuid,
    /// Public Ed25519 verifying key.
    pub verifying_key: VerifyingKey,
    /// Human-readable device label.
    pub label: String,
    /// Privacy-preserving fingerprint claim.
    pub fingerprint_hint: DeviceFingerprintHint,
    /// Current trust state.
    pub trust_state: DeviceTrustState,
    /// Creation timestamp.
    pub created_at: OffsetDateTime,
    /// Optional revocation timestamp.
    pub revoked_at: Option<OffsetDateTime>,
    /// Key generation counter for rotation.
    pub key_generation: u32,
}

/// Privacy-preserving fingerprint fields. Raw hardware identifiers are banned.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceFingerprintHint {
    /// Operating system family.
    pub os_family: String,
    /// App channel such as stable, beta, or enterprise.
    pub app_channel: String,
    /// User-visible coarse device kind.
    pub device_kind: String,
}

/// Signed registration request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceRegistration {
    /// Public device identity.
    pub identity: DeviceIdentity,
    /// Server-provided challenge.
    pub challenge: Vec<u8>,
    /// Signature over deterministic registration bytes.
    pub signature: Signature,
}

impl DeviceRegistration {
    /// Creates a signed device registration.
    pub fn sign(
        signing_key: &SigningKey,
        mut identity: DeviceIdentity,
        challenge: Vec<u8>,
    ) -> Self {
        identity.verifying_key = signing_key.verifying_key();
        let signature = signing_key.sign(&registration_message(&identity, &challenge));
        Self {
            identity,
            challenge,
            signature,
        }
    }

    /// Verifies registration proof-of-possession.
    pub fn verify(&self) -> Result<(), DeviceTrustError> {
        self.identity
            .verifying_key
            .verify(
                &registration_message(&self.identity, &self.challenge),
                &self.signature,
            )
            .map_err(|_| DeviceTrustError::InvalidSignature)
    }
}

/// Generates a new device signing key.
#[must_use]
pub fn generate_device_signing_key() -> SigningKey {
    SigningKey::generate(&mut OsRng)
}

fn registration_message(identity: &DeviceIdentity, challenge: &[u8]) -> Vec<u8> {
    format!(
        "espass:device-registration:v1\nuser={}\ndevice={}\ngeneration={}\n",
        identity.user_id, identity.device_id, identity.key_generation
    )
    .bytes()
    .chain(challenge.iter().copied())
    .collect()
}

/// Minimal trusted device store contract for desktop/backend implementations.
pub trait TrustedDeviceStore {
    /// Adds or replaces a trusted device identity.
    fn upsert_device(&mut self, identity: DeviceIdentity) -> Result<(), DeviceTrustError>;
    /// Returns a device by ID.
    fn get_device(&self, device_id: Uuid) -> Result<Option<DeviceIdentity>, DeviceTrustError>;
    /// Marks a device as revoked.
    fn revoke_device(
        &mut self,
        device_id: Uuid,
        revoked_at: OffsetDateTime,
    ) -> Result<(), DeviceTrustError>;
}

/// Device trust errors.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DeviceTrustError {
    /// Device signature verification failed.
    #[error("invalid device signature")]
    InvalidSignature,
    /// Device is revoked.
    #[error("device is revoked")]
    DeviceRevoked,
    /// Store operation failed.
    #[error("trusted device store error")]
    StoreError,
}
