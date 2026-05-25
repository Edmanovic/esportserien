//! Native messaging and desktop-extension IPC schemas.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use time::OffsetDateTime;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Browser extension platform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExtensionPlatform {
    /// Chromium MV3 extension.
    ChromiumMv3,
    /// Firefox WebExtension.
    FirefoxWebExtension,
}

/// Extension handshake request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionHandshake {
    /// Browser-reported extension origin.
    pub extension_origin: String,
    /// Extension platform.
    pub platform: ExtensionPlatform,
    /// Extension version.
    pub extension_version: String,
    /// Random extension nonce.
    pub extension_nonce: [u8; 32],
    /// Requested protocol version.
    pub protocol_version: u16,
}

/// Established IPC session metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IpcSession {
    /// IPC session UUID.
    pub session_id: Uuid,
    /// Pinned extension origin.
    pub extension_origin: String,
    /// Protocol version.
    pub protocol_version: u16,
    /// Creation timestamp.
    pub created_at: OffsetDateTime,
    /// Expiration timestamp.
    pub expires_at: OffsetDateTime,
    /// Last accepted nonce counter.
    pub last_counter: u64,
}

/// Signed IPC message envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedMessageEnvelope<T> {
    /// IPC session UUID.
    pub session_id: Uuid,
    /// Correlation ID for request/response matching.
    pub correlation_id: Uuid,
    /// Strictly increasing sender counter.
    pub counter: u64,
    /// Message timestamp.
    pub sent_at: OffsetDateTime,
    /// Typed payload.
    pub payload: T,
    /// HMAC-SHA256 over canonical message fields.
    pub signature: [u8; 32],
}

impl<T> SignedMessageEnvelope<T>
where
    T: Serialize,
{
    /// Signs an IPC envelope with an ephemeral session key.
    pub fn sign(
        session_id: Uuid,
        correlation_id: Uuid,
        counter: u64,
        sent_at: OffsetDateTime,
        payload: T,
        session_key: &[u8],
    ) -> Result<Self, IpcError> {
        let signature = ipc_signature(
            session_id,
            correlation_id,
            counter,
            sent_at,
            &payload,
            session_key,
        )?;
        Ok(Self {
            session_id,
            correlation_id,
            counter,
            sent_at,
            payload,
            signature,
        })
    }

    /// Verifies the IPC envelope signature.
    pub fn verify(&self, session_key: &[u8]) -> Result<(), IpcError> {
        let mut mac = HmacSha256::new_from_slice(session_key).map_err(|_| IpcError::InvalidKey)?;
        mac.update(b"espass:ipc:v1\n");
        mac.update(self.session_id.as_bytes());
        mac.update(self.correlation_id.as_bytes());
        mac.update(&self.counter.to_be_bytes());
        mac.update(&self.sent_at.unix_timestamp_nanos().to_be_bytes());
        let payload_bytes =
            serde_json::to_vec(&self.payload).map_err(|_| IpcError::InvalidPayload)?;
        mac.update(&payload_bytes);
        mac.verify_slice(&self.signature)
            .map_err(|_| IpcError::InvalidSignature)
    }
}

fn ipc_signature<T: Serialize>(
    session_id: Uuid,
    correlation_id: Uuid,
    counter: u64,
    sent_at: OffsetDateTime,
    payload: &T,
    session_key: &[u8],
) -> Result<[u8; 32], IpcError> {
    let mut mac = HmacSha256::new_from_slice(session_key).map_err(|_| IpcError::InvalidKey)?;
    mac.update(b"espass:ipc:v1\n");
    mac.update(session_id.as_bytes());
    mac.update(correlation_id.as_bytes());
    mac.update(&counter.to_be_bytes());
    mac.update(&sent_at.unix_timestamp_nanos().to_be_bytes());
    let payload_bytes = serde_json::to_vec(payload).map_err(|_| IpcError::InvalidPayload)?;
    mac.update(&payload_bytes);
    Ok(mac.finalize().into_bytes().into())
}

/// Extension access decision model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IpcPermissionModel {
    /// Exact extension origins allowed to connect.
    pub allowed_extension_origins: Vec<String>,
    /// Maximum message size accepted from extension.
    pub max_message_bytes: usize,
    /// Whether content-script initiated autofill requires user gesture.
    pub require_user_gesture_for_autofill: bool,
}

impl IpcPermissionModel {
    /// Returns true when the extension origin is pinned and exact.
    #[must_use]
    pub fn allows_origin(&self, origin: &str) -> bool {
        self.allowed_extension_origins
            .iter()
            .any(|allowed| allowed == origin)
    }
}

/// Native messaging protocol messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum IpcPayload {
    /// Begin extension handshake.
    Handshake(ExtensionHandshake),
    /// Request credentials for a validated origin.
    CredentialRequest {
        /// Page origin.
        origin: String,
        /// Top-level origin.
        top_level_origin: String,
        /// Whether the request came from a user gesture.
        user_gesture: bool,
    },
    /// Deny or grant a per-site permission.
    PermissionDecision {
        /// Site origin.
        origin: String,
        /// Grant decision.
        granted: bool,
    },
    /// Error response.
    Error {
        /// Stable error code.
        code: String,
        /// Redacted message.
        message: String,
    },
}

/// IPC protocol errors.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IpcError {
    /// Signature was invalid.
    #[error("invalid IPC signature")]
    InvalidSignature,
    /// Session key was invalid.
    #[error("invalid IPC session key")]
    InvalidKey,
    /// Payload could not be serialized or validated.
    #[error("invalid IPC payload")]
    InvalidPayload,
    /// Message replay was detected.
    #[error("IPC replay detected")]
    ReplayDetected,
    /// Extension origin is not pinned.
    #[error("extension origin is not allowed")]
    OriginDenied,
}
