# ESPASS Trust Boundary Map

## Boundary 1: Extension Content Script → Service Worker

**Direction:** one-way message  
**Transport:** `chrome.runtime.sendMessage`  
**Validation:** Service worker verifies `sender.url` origin matches `message.origin`  
**Secrets crossing boundary:** None. Only origin metadata crosses this boundary.

## Boundary 2: Browser Extension → Native Messaging Host

**Direction:** bidirectional, length-prefixed JSON  
**Transport:** Chrome/Firefox native messaging  
**Validation:**  
- Origin pinned to `ESPASS_ALLOWED_EXTENSION_ORIGINS` env var  
- Extension handshake verified before accepting any other message  
- All subsequent messages require a signed `SignedMessageEnvelope` with HMAC-SHA256  
- Monotonic counter enforced to prevent replay  
**Secrets crossing boundary:** Ephemeral session key confirmation (once, during handshake). Decrypted credentials cross this boundary during autofill — this is the highest-risk data flow.

## Boundary 3: Tauri Renderer → Tauri Rust Core

**Direction:** bidirectional via Tauri IPC  
**Transport:** Tauri's internal WebView IPC  
**Validation:**  
- Commands are allowlisted via `tauri::generate_handler!`  
- All commands wrap in `catch_vault_panic`  
- All errors sanitized via `sanitize_error` before reaching renderer  
**Secrets crossing boundary:** Password bytes on unlock (immediately zeroized). Session status (boolean only). The vault key never crosses this boundary.

## Boundary 4: Desktop App → Backend Sync API

**Direction:** client-initiated  
**Transport:** HTTPS (TLS 1.3)  
**Validation:** Backend receives and stores only ciphertext  
**Secrets crossing boundary:** None. Only encrypted blobs and metadata.

## Assumptions

1. The native messaging host binary is not tampered with post-installation.
2. The OS prevents other processes from reading the host's memory.
3. The user's machine is not compromised at the OS level.
4. The browser sandbox is not fully compromised.
