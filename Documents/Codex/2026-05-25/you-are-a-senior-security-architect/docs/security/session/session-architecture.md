# Session Architecture

ESPASS sessions are short-lived, device-bound, replay-protected, and revocable.

## Authentication Flow

1. User authenticates with password-derived unlock and phishing-resistant factors where configured.
2. Device identity proof binds the login to a registered device.
3. Backend issues a short-lived access session and refresh token family.
4. Refresh tokens rotate on every use; reuse indicates theft and revokes the family.

## Unlock Flow

Vault unlock is local. The user enters the master password, `crypto-core` derives the master key using Argon2id, and the client decrypts vault key slots locally. The server is not involved in plaintext unlock.

## Offline Unlock

Offline unlock uses encrypted local cache and locally stored KDF parameters. Sync resumes later with conflict detection. Expired online sessions do not prevent local decryption, but policy may require periodic online re-authentication before sync.

## Expiration

- Access session lifetime: 15 minutes by default.
- Idle timeout: 15 minutes by default.
- Absolute timeout: 12 hours by default.
- Request counters must advance monotonically to prevent replay.

