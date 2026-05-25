# STRIDE Threat Model

## Scope

This model covers the desktop app, browser extension, backend API, encrypted synchronization, admin APIs, and internal security automation.

## Assets

| Asset | Impact If Compromised |
| --- | --- |
| Master password | Offline vault decryption risk if encrypted blobs are also obtained |
| Vault DEKs | Full vault compromise |
| Vault items | Direct credential disclosure |
| TOTP seeds | MFA bypass risk |
| Device private keys | Impersonation and sharing compromise |
| Sessions/refresh tokens | Account takeover until revoked |
| Audit logs | Forensics loss or privacy exposure |
| Admin policies | Enterprise-wide control bypass |

## STRIDE Summary

| Category | Example Threats | Controls |
| --- | --- | --- |
| Spoofing | Fake backend, phishing domain, malicious extension message sender | TLS, pinning where appropriate, WebAuthn, origin validation, signed IPC messages |
| Tampering | Modified encrypted blob, downgraded KDF params, altered extension bundle | AEAD tags, signed releases, schema validation, minimum KDF policy |
| Repudiation | Admin denies policy change, user denies vault export | Append-only audit logs, server-side event signing, device IDs |
| Information Disclosure | Plaintext in logs, content script leakage, memory scraping | Secret redaction, isolated extension contexts, auto-lock, zeroization, OS secure storage |
| Denial of Service | Sync flooding, login brute force, large blob upload | Rate limits, quotas, backpressure, lockouts, circuit breakers |
| Elevation of Privilege | Team role bypass, extension over-permission, insecure IPC | OPA policies, least privilege permissions, authenticated IPC, authorization tests |

## Priority Abuse Cases

1. Attacker steals backend database and object storage.
   - Expected result: only encrypted blobs and non-secret metadata are exposed.
   - Required controls: strong Argon2id parameters, random salts, AEAD, no plaintext secrets.

2. Malicious website attempts to trick extension autofill.
   - Expected result: fill is blocked unless effective top-level origin matches saved item policy.
   - Required controls: origin normalization, iframe restrictions, phishing heuristics, user confirmation for sensitive fills.

3. Local malware tries to read unlocked vault memory.
   - Expected result: risk reduced but not eliminated.
   - Required controls: auto-lock, secure memory handling, short-lived plaintext buffers, OS keychain, process hardening.

4. Compromised admin attempts to read user vault data.
   - Expected result: impossible without user-side keys.
   - Required controls: zero-knowledge design, admin APIs limited to metadata/policy.

## Residual Risks

- A fully compromised endpoint can read secrets while a vault is unlocked.
- Browser DOM automation is inherently exposed to deceptive UI and compromised pages.
- Password strength remains user-dependent unless enterprise policy enforces strong master password and hardware-backed unlock.

