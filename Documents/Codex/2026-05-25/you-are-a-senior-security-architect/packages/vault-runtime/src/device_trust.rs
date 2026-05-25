//! Device trust execution runtime.

use std::collections::BTreeMap;

use espass_shared_types::device::{
    DeviceIdentity, DeviceRegistration, DeviceTrustError, DeviceTrustState, TrustedDeviceStore,
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::RuntimeError;

/// In-memory trusted device registry suitable for prototype runtime tests.
#[derive(Debug, Clone, Default)]
pub struct TrustedDeviceRegistry {
    devices: BTreeMap<Uuid, DeviceIdentity>,
}

impl TrustedDeviceRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true when the device is trusted and not revoked.
    #[must_use]
    pub fn is_trusted(&self, device_id: Uuid) -> bool {
        self.devices
            .get(&device_id)
            .is_some_and(|device| device.trust_state == DeviceTrustState::Trusted)
    }

    /// Applies a key rotation by replacing the identity with a higher generation.
    pub fn rotate_device(&mut self, identity: DeviceIdentity) -> Result<(), RuntimeError> {
        let existing = self
            .devices
            .get(&identity.device_id)
            .ok_or(RuntimeError::DeviceTrust)?;
        if existing.trust_state == DeviceTrustState::Revoked
            || identity.key_generation <= existing.key_generation
        {
            return Err(RuntimeError::DeviceTrust);
        }
        self.devices.insert(identity.device_id, identity);
        Ok(())
    }
}

impl TrustedDeviceStore for TrustedDeviceRegistry {
    fn upsert_device(&mut self, identity: DeviceIdentity) -> Result<(), DeviceTrustError> {
        self.devices.insert(identity.device_id, identity);
        Ok(())
    }

    fn get_device(&self, device_id: Uuid) -> Result<Option<DeviceIdentity>, DeviceTrustError> {
        Ok(self.devices.get(&device_id).cloned())
    }

    fn revoke_device(
        &mut self,
        device_id: Uuid,
        revoked_at: OffsetDateTime,
    ) -> Result<(), DeviceTrustError> {
        let device = self
            .devices
            .get_mut(&device_id)
            .ok_or(DeviceTrustError::StoreError)?;
        device.trust_state = DeviceTrustState::Revoked;
        device.revoked_at = Some(revoked_at);
        Ok(())
    }
}

/// Device trust runtime.
#[derive(Debug, Clone, Default)]
pub struct DeviceTrustRuntime {
    registry: TrustedDeviceRegistry,
}

impl DeviceTrustRuntime {
    /// Creates an empty runtime.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Approves a signed registration after proof-of-possession verification.
    pub fn approve_registration(
        &mut self,
        registration: DeviceRegistration,
    ) -> Result<DeviceIdentity, RuntimeError> {
        registration
            .verify()
            .map_err(|_| RuntimeError::DeviceTrust)?;
        let mut identity = registration.identity;
        identity.trust_state = DeviceTrustState::Trusted;
        self.registry
            .upsert_device(identity.clone())
            .map_err(|_| RuntimeError::DeviceTrust)?;
        Ok(identity)
    }

    /// Validates that a device may access runtime sync/autofill functions.
    pub fn require_trusted(&self, device_id: Uuid) -> Result<(), RuntimeError> {
        if self.registry.is_trusted(device_id) {
            Ok(())
        } else {
            Err(RuntimeError::DeviceTrust)
        }
    }

    /// Borrows the registry.
    #[must_use]
    pub fn registry(&self) -> &TrustedDeviceRegistry {
        &self.registry
    }
}

/// Revocation manager for compromised devices.
pub struct DeviceRevocationManager;

impl DeviceRevocationManager {
    /// Revokes a trusted device and forces later validation to fail.
    pub fn revoke(
        registry: &mut TrustedDeviceRegistry,
        device_id: Uuid,
        now: OffsetDateTime,
    ) -> Result<(), RuntimeError> {
        registry
            .revoke_device(device_id, now)
            .map_err(|_| RuntimeError::DeviceTrust)
    }
}
