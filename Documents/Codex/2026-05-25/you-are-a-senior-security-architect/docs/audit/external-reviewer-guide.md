# External Security Reviewer Guide

## Review Entry Points

- Cryptography: `packages/crypto-core`
- Runtime secrets and persistence: `packages/vault-runtime`
- Server-safe schemas: `packages/shared-types`
- Native messaging: `apps/desktop/native-messaging-host`
- Extension origin checks: `apps/extension/src`
- Backend encrypted sync: `apps/backend`

## Security Questions To Validate

1. Can any backend path receive or infer plaintext vault contents?
2. Can a malicious extension message bypass handshake, origin pinning, counters, or TTL?
3. Can corrupted local storage be accepted as valid?
4. Can renderer compromise invoke privileged desktop commands without capability grants?
5. Can credential autofill occur in a hidden field, cross-origin iframe, or spoofed domain?
6. Do key types prevent accidental cloning and debug leakage?
7. Does every failure mode fail closed?

## Known Review Notes

- Memory locking is best-effort and explicitly documented as residual risk.
- Production IPC should replace session-key confirmation with stronger channel binding.
- Release signing keys are not present in this repository and must be managed separately.

