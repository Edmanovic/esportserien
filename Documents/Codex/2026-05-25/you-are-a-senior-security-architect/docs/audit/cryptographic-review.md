# ESPASS Cryptographic Review Package

## Algorithms

| Purpose | Algorithm | Key Size | Reference |
|---------|-----------|----------|-----------|
| Vault encryption | AES-256-GCM | 256-bit | NIST SP 800-38D |
| Key derivation | Argon2id | 256-bit output | RFC 9106 |
| IPC message authentication | HMAC-SHA256 | 256-bit session key | RFC 2104 |
| Vault integrity tag | HMAC-SHA256 | Vault key | RFC 2104 |
| Device identity | Ed25519 | 256-bit | RFC 8032 |

## Key Derivation Parameters (Default)

```
Memory: 190 MiB (194,560 KiB)
Iterations: 3
Parallelism: 1
Salt: 128-bit random
Output: 256-bit MasterKey
```

Rationale: Exceeds OWASP ASVS 4.0 Level 2 requirements (64 MiB / 3 iterations minimum).

## Nonce Strategy

AES-256-GCM uses a 96-bit random nonce from `OsRng`. The probability of nonce collision is negligible given that each vault session generates a new VaultKey; the nonce birthday bound (2^48 messages) is never approached in practice.

## AAD Binding

Every encrypted payload binds tenant ID, vault ID, optional item ID, record type, and schema version as associated data. This prevents cross-vault, cross-tenant, and cross-type ciphertext substitution attacks.

## Residual Cryptographic Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| GCM nonce reuse under random generation | Low | Keys rotated per session; nonce space is 2^96 |
| Argon2id timing oracle via failed unlock | Low | Exponential backoff; constant-time password check not applicable (KDF is not constant-time by design) |
| AAD oracle via error message | Mitigated | All decryption errors return `DecryptionFailed` with no AAD detail |
