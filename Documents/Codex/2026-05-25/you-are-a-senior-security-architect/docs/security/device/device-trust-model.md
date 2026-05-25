# Device Trust Model

Each device owns an Ed25519 signing key. The private key remains local and is protected by OS secure storage where available. The backend stores public identity, trust state, key generation, and revocation metadata.

## Trust Establishment

1. Server issues a registration challenge.
2. Device creates or loads its signing key.
3. Device signs `user_id`, `device_id`, key generation, and challenge.
4. Server verifies proof-of-possession and stores the verifying key.
5. Existing trusted device or enterprise policy approves trust.

## Compromised Device Flow

- Mark device revoked server-side.
- Reject sync and session refresh for the device.
- Rotate affected vault key slots.
- Notify users and admins through audit-safe events.

## Key Rotation

Rotation creates a new device verifying key with an incremented generation. The old key remains valid only for signing the rotation request until the new key is accepted. Revoked keys cannot rotate.

## Fingerprinting Safeguards

ESPASS stores coarse fingerprint hints only. Raw hardware serials, stable advertising IDs, MAC addresses, and invasive fingerprints are prohibited.

