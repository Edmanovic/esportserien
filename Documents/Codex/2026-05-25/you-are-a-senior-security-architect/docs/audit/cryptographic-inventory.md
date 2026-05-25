# Cryptographic Inventory

| Use | Primitive | Implementation | Notes |
| --- | --- | --- | --- |
| Master password KDF | Argon2id | `argon2` crate | Enforces minimum memory/iteration policy |
| Vault payload encryption | AES-256-GCM | `aes-gcm` crate | Random 96-bit nonce, associated data binding |
| Local vault integrity | HMAC-SHA256 | `hmac` + `sha2` | Covers deterministic local record fields |
| IPC envelope signing | HMAC-SHA256 | `hmac` + `sha2` | Ephemeral session key, counters, TTL |
| Device registration | Ed25519 | `ed25519-dalek` | Proof-of-possession for device identity |
| Randomness | OS CSPRNG | `rand_core::OsRng` | Used for salts, nonces, sessions |

