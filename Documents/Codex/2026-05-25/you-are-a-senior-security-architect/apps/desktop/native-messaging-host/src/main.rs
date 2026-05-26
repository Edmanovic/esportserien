//! ESPASS native messaging host — stdio ↔ WebSocket bridge.
//!
//! Chrome spawns this binary and communicates over stdin/stdout using the
//! native messaging length-prefix protocol (4-byte LE u32 + UTF-8 JSON).
//! This host reads `%APPDATA%\espass\ipc.port` and connects to the
//! Tauri desktop app's local WebSocket IPC server, then forwards messages
//! in both directions until stdin closes or the WebSocket disconnects.

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() {
    if let Err(code) = run().await {
        // Send a structured error to the browser so the extension can react.
        let msg = serde_json::to_vec(&serde_json::json!({
            "type": "error",
            "code": code,
        }))
        .unwrap_or_default();
        let mut stdout = tokio::io::stdout();
        let _ = stdout.write_all(&(msg.len() as u32).to_le_bytes()).await;
        let _ = stdout.write_all(&msg).await;
        let _ = stdout.flush().await;
        std::process::exit(1);
    }
}

async fn run() -> Result<(), &'static str> {
    let port = read_ipc_port()?;
    let url = format!("ws://127.0.0.1:{port}");

    let (ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|_| "desktop-unavailable")?;

    let (mut ws_sink, mut ws_stream) = ws.split();
    let mut stdin  = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    loop {
        tokio::select! {
            // stdin (Chrome) → WebSocket (Tauri)
            result = read_native_msg(&mut stdin) => {
                match result.map_err(|_| "stdin-read-error")? {
                    Some(json) => {
                        ws_sink
                            .send(Message::Text(json.into()))
                            .await
                            .map_err(|_| "websocket-write-error")?;
                    }
                    None => break, // browser closed the connection
                }
            }
            // WebSocket (Tauri) → stdout (Chrome)
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        write_native_msg(&mut stdout, text.as_str())
                            .await
                            .map_err(|_| "stdout-write-error")?;
                    }
                    None | Some(Err(_)) => break, // server closed
                    _ => {} // binary/ping frames — ignore
                }
            }
        }
    }

    Ok(())
}

/// Read the IPC port from the file written by the Tauri desktop app.
/// Windows-only: uses `%APPDATA%\espass\ipc.port`.
/// See `AppState::default()` in the desktop crate for cross-platform path logic.
fn read_ipc_port() -> Result<u16, &'static str> {
    let appdata = std::env::var("APPDATA").map_err(|_| "desktop-unavailable")?;
    let path = std::path::Path::new(&appdata)
        .join("espass")
        .join("ipc.port");
    let text = std::fs::read_to_string(&path).map_err(|_| "desktop-unavailable")?;
    text.trim().parse::<u16>().map_err(|_| "desktop-unavailable")
}

/// Read one length-prefixed message from `reader`.
/// Returns `Ok(None)` when stdin is closed cleanly.
async fn read_native_msg<R>(reader: &mut R) -> std::io::Result<Option<String>>
where
    R: AsyncReadExt + Unpin,
{
    let mut len_bytes = [0u8; 4];
    match reader.read_exact(&mut len_bytes).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_bytes) as usize;
    if len > 1024 * 1024 {
        return Err(std::io::Error::other("message too large"));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    String::from_utf8(buf)
        .map(Some)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Write one length-prefixed message to `writer`.
async fn write_native_msg<W>(writer: &mut W, text: &str) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let bytes = text.as_bytes();
    if bytes.len() > 1024 * 1024 {
        return Err(std::io::Error::other("message too large"));
    }
    writer.write_all(&(bytes.len() as u32).to_le_bytes()).await?;
    writer.write_all(bytes).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn native_msg_round_trip() {
        let text = r#"{"type":"ping","request_id":"abc-123"}"#;
        let mut buf: Vec<u8> = Vec::new();
        write_native_msg(&mut buf, text).await.unwrap();
        let mut cursor = std::io::Cursor::new(buf);
        let decoded = read_native_msg(&mut cursor).await.unwrap();
        assert_eq!(decoded, Some(text.to_string()));
    }

    #[tokio::test]
    async fn empty_stdin_returns_none() {
        let mut cursor = std::io::Cursor::new(vec![]);
        let result = read_native_msg(&mut cursor).await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn read_rejects_oversized_message() {
        // A length header of 1_048_577 (> 1 MB) must return Err, not allocate.
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(&1_048_577u32.to_le_bytes()); // 4-byte LE header
        let mut cursor = std::io::Cursor::new(buf);
        let result = read_native_msg(&mut cursor).await;
        assert!(result.is_err(), "oversized message should return Err");
    }

    #[tokio::test]
    async fn write_rejects_oversized_message() {
        // A text string of 1_048_577 bytes must return Err on write.
        let oversized = "x".repeat(1_048_577);
        let mut buf: Vec<u8> = Vec::new();
        let result = write_native_msg(&mut buf, &oversized).await;
        assert!(result.is_err(), "writing oversized message should return Err");
    }

    #[tokio::test]
    async fn read_rejects_invalid_utf8() {
        // Valid 4-byte length header pointing to invalid UTF-8 bytes.
        let body: Vec<u8> = vec![0xFF, 0xFE, 0x00, 0x01]; // invalid UTF-8
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
        buf.extend_from_slice(&body);
        let mut cursor = std::io::Cursor::new(buf);
        let result = read_native_msg(&mut cursor).await;
        assert!(result.is_err(), "invalid UTF-8 should return Err");
        if let Err(e) = result {
            assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
        }
    }
}
