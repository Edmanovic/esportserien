# ESPASS Crypto Core

`espass-crypto-core` contains the first security-critical primitives for ESPASS:

- Argon2id password-based master-key derivation.
- AES-256-GCM authenticated encryption.
- Versioned encrypted envelope metadata.
- Typed `MasterKey`, `VaultKey`, `DeviceKey`, and `SessionKey` boundaries.
- Best-effort zeroization and memory locking for derived keys and plaintext buffers.

The crate deliberately does not implement custom cryptography. It wraps established RustCrypto crates with ESPASS-specific validation, defaults, and envelope formatting.

## Security Notes

- Use a unique random salt per account or vault KDF context.
- Use stable associated data that binds ciphertext to tenant, vault, item, and record type.
- Never reuse a nonce with the same key. This crate generates random nonces for every encryption operation.
- Store `EncryptedEnvelope` server-side; keep `SecretKey` and `SensitiveBytes` in trusted client memory only.
