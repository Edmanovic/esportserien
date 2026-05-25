# Auth Package

Authentication and session helpers for ESPASS clients and services.

Planned responsibilities:

- WebAuthn/FIDO2 registration and authentication flows.
- Session token handling with refresh token rotation.
- Client-side vault unlock orchestration.
- Recovery key and enterprise recovery policy support.

This package may depend on `crypto-core`; it must not expose master passwords or plaintext keys to backend APIs.

