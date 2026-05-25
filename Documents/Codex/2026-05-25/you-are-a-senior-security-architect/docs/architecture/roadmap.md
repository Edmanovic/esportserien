# Secure Development Roadmap

## Phase 1: Design and Repository Foundation

- High-level architecture.
- STRIDE threat model.
- Cryptographic design.
- ADRs for major technology choices.
- Repository skeleton.
- Internal security agent framework.

## Phase 2: Security Core and Backend Foundation

- `crypto-core` password KDF and AEAD envelope.
- Vault key hierarchy package.
- Backend user, device, vault metadata, and encrypted blob schema.
- Session and rate limiting.
- API contract tests.

## Phase 3: Desktop and Extension MVP

- Tauri desktop locked/unlocked shell.
- Encrypted offline cache.
- Password generator and basic vault CRUD.
- Browser extension detection and manual autofill.
- Secure desktop-extension IPC.

## Phase 4: Security Hardening

- Fuzzing for envelope parsing and API validation.
- SAST, dependency audit, secret scanning.
- DAST against local backend.
- Red-team abuse case suite.
- CSP and sandbox hardening.

## Phase 5: Enterprise Release Readiness

- Signed desktop and extension releases.
- Reproducible build documentation.
- Kubernetes deployment.
- Monitoring, backups, and disaster recovery.
- Compliance evidence mapping.

