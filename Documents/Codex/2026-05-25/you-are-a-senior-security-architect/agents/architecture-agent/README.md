# Architecture Agent

Enforces:

- Dependency direction.
- No backend imports of client plaintext vault models.
- Crypto isolation in `packages/crypto-core`.
- Clear API boundaries between extension content scripts, background scripts, and desktop IPC.

