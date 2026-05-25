# ESPASS External Audit Scope

**Version:** 0.1.0-pre-audit  
**Date:** 2026-05-25  
**Classification:** Restricted — For Security Reviewers Only

## Scope

### In Scope
- `packages/crypto-core` — AES-256-GCM, Argon2id KDF, key types, memory locking, secure buffer
- `packages/vault-runtime` — unlock lifecycle, session management, persistence, IPC, autofill, device trust, panic boundary, secret windows
- `packages/shared-types` — vault schema V1, IPC protocol, device identity, autofill policy
- `apps/desktop/native-messaging-host` — browser native messaging protocol implementation
- `apps/extension/src/` — content script autofill guard, service worker, overlay detection
- `apps/desktop/src-tauri/src/` — Tauri command handlers and state management
- `apps/backend/src/` — zero-knowledge sync API

### Out of Scope for This Review
- Frontend UI components (not yet implemented)
- Mobile applications (not yet implemented)
- Enterprise RBAC (not yet implemented)
- SSO integrations (not yet implemented)
- Cloud deployment infrastructure

## Review Focus Areas

1. **Cryptographic correctness** — key derivation, encryption, AAD binding, nonce handling
2. **Memory safety** — secret zeroization, memory locking, panic safety
3. **IPC protocol** — handshake, signature verification, replay protection, origin pinning
4. **Vault integrity** — schema validation, integrity tags, migration safety
5. **Autofill security** — origin validation, overlay detection, punycode detection
6. **Error handling** — information leakage across trust boundaries
7. **Dependency supply chain** — critical cryptographic dependencies
