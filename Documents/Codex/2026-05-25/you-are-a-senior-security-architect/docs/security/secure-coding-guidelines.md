# Secure Coding Guidelines

## Secrets

- Never log secrets, ciphertext keys, TOTP seeds, recovery codes, session tokens, or authorization headers.
- Use redaction wrappers for any value that may contain sensitive material.
- Keep plaintext lifetimes short and zeroize buffers where practical.
- Do not store secrets in browser `localStorage`.

## Cryptography

- Do not introduce new primitives without an ADR.
- Do not use MD5, SHA1, ECB mode, static IVs, or unauthenticated encryption.
- Include associated data for encrypted records.
- Reject downgraded or unknown KDF/envelope versions.

## Input Handling

- Validate all client input at trust boundaries.
- Normalize and validate domains before autofill decisions.
- Treat metadata as attacker-controlled even when authenticated.

## Frontend

- Enforce strict CSP.
- Avoid `dangerouslySetInnerHTML`.
- Keep extension content scripts free of persistent secrets.
- Use explicit message schemas for extension and desktop IPC.

