//! Desktop-extension IPC runtime.

use std::collections::BTreeMap;

use espass_crypto_core::{random_array, SessionKey};
use espass_shared_types::ipc::{
    ExtensionHandshake, IpcError, IpcPermissionModel, IpcSession, SignedMessageEnvelope,
};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::RuntimeError;

/// Signed IPC request alias.
pub type SignedIpcRequest<T> = SignedMessageEnvelope<T>;

/// Validates pinned extension origins and protocol versions.
#[derive(Debug, Clone)]
pub struct ExtensionTrustValidator {
    permission_model: IpcPermissionModel,
    protocol_version: u16,
}

impl ExtensionTrustValidator {
    /// Creates an extension trust validator.
    #[must_use]
    pub fn new(permission_model: IpcPermissionModel, protocol_version: u16) -> Self {
        Self {
            permission_model,
            protocol_version,
        }
    }

    /// Validates handshake invariants.
    pub fn validate(&self, handshake: &ExtensionHandshake) -> Result<(), RuntimeError> {
        if handshake.protocol_version != self.protocol_version
            || !self
                .permission_model
                .allows_origin(&handshake.extension_origin)
        {
            return Err(RuntimeError::Ipc);
        }
        Ok(())
    }
}

/// Handshake response with desktop challenge and ephemeral session key material.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandshakeAccepted {
    /// New IPC session.
    pub session: IpcSession,
    /// Desktop challenge nonce.
    pub desktop_nonce: [u8; 32],
    /// Ephemeral session key encrypted by native channel policy in future builds.
    pub session_key_confirmation: [u8; 32],
}

/// IPC handshake manager.
#[derive(Debug, Clone)]
pub struct IpcHandshakeManager {
    validator: ExtensionTrustValidator,
    ttl_seconds: i64,
}

impl IpcHandshakeManager {
    /// Creates a handshake manager.
    #[must_use]
    pub fn new(validator: ExtensionTrustValidator) -> Self {
        Self {
            validator,
            ttl_seconds: 300,
        }
    }

    /// Accepts a validated extension handshake and creates an IPC session.
    pub fn accept(
        &self,
        handshake: &ExtensionHandshake,
        now: OffsetDateTime,
    ) -> Result<(HandshakeAccepted, SessionKey), RuntimeError> {
        self.validator.validate(handshake)?;
        let session_key = SessionKey::from_bytes(random_array()?);
        let desktop_nonce = random_array()?;
        let session = IpcSession {
            session_id: Uuid::new_v4(),
            extension_origin: handshake.extension_origin.clone(),
            protocol_version: handshake.protocol_version,
            created_at: now,
            expires_at: now + Duration::seconds(self.ttl_seconds),
            last_counter: 0,
        };
        let accepted = HandshakeAccepted {
            session,
            desktop_nonce,
            session_key_confirmation: *session_key.expose_secret(),
        };
        Ok((accepted, session_key))
    }
}

/// Runtime IPC session entry.
pub struct IpcSessionEntry {
    session: IpcSession,
    session_key: SessionKey,
}

/// IPC session registry with expiration and replay protection.
#[derive(Default)]
pub struct IpcSessionRegistry {
    sessions: BTreeMap<Uuid, IpcSessionEntry>,
}

impl IpcSessionRegistry {
    /// Creates an empty session registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a session.
    pub fn insert(&mut self, session: IpcSession, session_key: SessionKey) {
        self.sessions.insert(
            session.session_id,
            IpcSessionEntry {
                session,
                session_key,
            },
        );
    }

    /// Validates signature, expiration, and replay counter.
    pub fn validate<T>(
        &mut self,
        request: &SignedMessageEnvelope<T>,
        now: OffsetDateTime,
    ) -> Result<(), RuntimeError>
    where
        T: Serialize,
    {
        let entry = self
            .sessions
            .get_mut(&request.session_id)
            .ok_or(RuntimeError::Ipc)?;
        if now >= entry.session.expires_at {
            return Err(RuntimeError::Session);
        }
        if request.counter <= entry.session.last_counter {
            return Err(RuntimeError::Ipc);
        }
        request
            .verify(entry.session_key.expose_secret())
            .map_err(|error| match error {
                IpcError::InvalidSignature => RuntimeError::Ipc,
                _ => RuntimeError::Ipc,
            })?;
        entry.session.last_counter = request.counter;
        Ok(())
    }
}
