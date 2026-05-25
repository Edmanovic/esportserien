# ESPASS Architecture Review Package

## System Overview

ESPASS is a zero-knowledge password manager. The server stores only encrypted blobs. Keys are derived client-side from the user's master password and never leave the device unencrypted.

## Component Interaction

```
Browser Extension (MV3)
    │  chrome.runtime.sendMessage (origin-validated)
    ▼
Service Worker
    │  chrome.runtime.connectNative
    ▼
Native Messaging Host (Rust)
    │  HMAC-signed JSON over stdin/stdout
    ▼
Vault Runtime (Rust library)
    ├── RuntimeSecretStore (mlock'd VaultKey, in-memory only)
    ├── UnlockManager (Argon2id → decrypt VaultKey)
    ├── IpcSessionRegistry (ephemeral session keys)
    └── SecureAutofillRuntime (origin validation → decrypt → fill)

Desktop App (Tauri)
    │  Tauri IPC (command allowlist)
    ▼
Vault Runtime (same Rust library)

Backend Sync API (Axum)
    │  HTTPS — receives ciphertext only
    ▼
Encrypted blob storage (server never holds keys)
```

## Security Properties

1. **Zero-knowledge sync:** The backend stores `VaultItem.encrypted_payload` blobs. It cannot decrypt them.
2. **Vault key isolation:** `VaultKey` lives only in `RuntimeSecretStore`, which zeroizes on drop and optionally locks pages with `mlock`/`VirtualLock`.
3. **Session-scoped IPC:** Each extension connection gets a fresh 256-bit `SessionKey`. Messages are HMAC-SHA256 signed with a monotonic counter. Sessions expire after 5 minutes.
4. **Autofill policy:** A credential is decrypted only when the requesting origin exactly matches the saved origin, the field is visible, no overlay is detected, and a user gesture is present.
5. **Panic isolation:** Every Tauri command and IPC handler wraps its body in `catch_vault_panic`. Panics are converted to `RuntimeError::InternalPanic` and never propagate to the renderer.
