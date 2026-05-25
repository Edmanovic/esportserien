//! ESPASS native messaging host.
//!
//! The host speaks the browser native messaging length-prefixed JSON protocol
//! over stdin/stdout. It performs only strict framing and extension-origin
//! validation here; vault access is delegated to the desktop vault service.

use std::io::{self, Read, Write};

use espass_shared_types::ipc::{IpcError, IpcPayload, IpcPermissionModel};
use espass_vault_runtime::{ExtensionTrustValidator, IpcHandshakeManager, IpcSessionRegistry};
use serde_json::Value;
use time::OffsetDateTime;

const MAX_BROWSER_MESSAGE_BYTES: usize = 1024 * 1024;

fn main() -> Result<(), HostError> {
    let caller_origin = std::env::args().nth(1).unwrap_or_default();
    let permission_model = IpcPermissionModel {
        allowed_extension_origins: configured_allowed_origins(),
        max_message_bytes: MAX_BROWSER_MESSAGE_BYTES,
        require_user_gesture_for_autofill: true,
    };

    if !permission_model.allows_origin(&caller_origin) {
        return Err(HostError::Ipc(IpcError::OriginDenied));
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let validator = ExtensionTrustValidator::new(permission_model.clone(), 1);
    let handshake_manager = IpcHandshakeManager::new(validator);
    let mut sessions = IpcSessionRegistry::new();

    while let Some(message) = read_native_message(&mut stdin, permission_model.max_message_bytes)? {
        let response = handle_message(&caller_origin, message, &handshake_manager, &mut sessions);
        write_native_message(&mut stdout, &response)?;
    }

    Ok(())
}

fn configured_allowed_origins() -> Vec<String> {
    std::env::var("ESPASS_ALLOWED_EXTENSION_ORIGINS")
        .unwrap_or_default()
        .split(',')
        .filter(|origin| !origin.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn handle_message(
    caller_origin: &str,
    message: Value,
    handshake_manager: &IpcHandshakeManager,
    sessions: &mut IpcSessionRegistry,
) -> Value {
    let payload = serde_json::from_value::<IpcPayload>(message);
    match payload {
        Ok(IpcPayload::Handshake(handshake)) if handshake.extension_origin == caller_origin => {
            match handshake_manager.accept(&handshake, OffsetDateTime::now_utc()) {
                Ok((accepted, session_key)) => {
                    sessions.insert(accepted.session.clone(), session_key);
                    serde_json::to_value(accepted).unwrap_or_else(|_| {
                        serde_json::json!({
                            "type": "error",
                            "code": "serialization-failed",
                            "message": "Handshake response failed"
                        })
                    })
                }
                Err(_) => serde_json::json!({
                    "type": "error",
                    "code": "handshake-denied",
                    "message": "Handshake failed trust validation"
                }),
            }
        }
        Ok(_) => serde_json::json!({
            "type": "error",
            "code": "unsupported-message",
            "message": "Message is not accepted before a signed IPC session is established"
        }),
        Err(_) => serde_json::json!({
            "type": "error",
            "code": "invalid-schema",
            "message": "Message failed strict schema validation"
        }),
    }
}

fn read_native_message<R: Read>(
    reader: &mut R,
    max_len: usize,
) -> Result<Option<Value>, HostError> {
    let mut len_bytes = [0_u8; 4];
    match reader.read_exact(&mut len_bytes) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(HostError::Io(error)),
    }

    let len = u32::from_ne_bytes(len_bytes) as usize;
    if len > max_len {
        return Err(HostError::MessageTooLarge);
    }

    let mut buffer = vec![0_u8; len];
    reader.read_exact(&mut buffer)?;
    let value = serde_json::from_slice(&buffer).map_err(|_| HostError::InvalidJson)?;
    Ok(Some(value))
}

fn write_native_message<W: Write>(writer: &mut W, value: &Value) -> Result<(), HostError> {
    let bytes = serde_json::to_vec(value).map_err(|_| HostError::InvalidJson)?;
    if bytes.len() > MAX_BROWSER_MESSAGE_BYTES {
        return Err(HostError::MessageTooLarge);
    }
    writer.write_all(&(bytes.len() as u32).to_ne_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum HostError {
    #[error("io error")]
    Io(#[from] io::Error),
    #[error("invalid json")]
    InvalidJson,
    #[error("message too large")]
    MessageTooLarge,
    #[error("ipc error")]
    Ipc(#[from] IpcError),
}

#[cfg(test)]
mod tests {
    use super::{read_native_message, write_native_message};

    #[test]
    fn native_message_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let value = serde_json::json!({"type":"ping"});
        let mut bytes = Vec::new();
        write_native_message(&mut bytes, &value)?;

        let mut cursor = std::io::Cursor::new(bytes);
        let decoded = read_native_message(&mut cursor, 1024)?
            .ok_or_else(|| std::io::Error::other("message should exist"))?;
        assert_eq!(decoded, value);
        Ok(())
    }
}
