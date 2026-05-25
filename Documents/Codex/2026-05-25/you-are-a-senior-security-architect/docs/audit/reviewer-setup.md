# ESPASS Reviewer Setup Guide

## Prerequisites

- Rust 1.95.0 (pinned via `rust-toolchain.toml`)
- Node.js 20+ (for extension TypeScript)
- Git

## Repository Setup

```bash
git clone <repo-url> espass
cd espass
rustup show  # Should print "1.95.0"
```

## Build and Test

```bash
# Build all Rust crates
cargo build --workspace

# Run all tests
cargo test --workspace

# Run with security lints
cargo clippy --workspace --all-targets -- -D warnings

# Check dependency advisories
cargo audit

# Check dependency policy
cargo deny check
```

## Fuzzing (requires nightly)

```bash
rustup install nightly
cargo +nightly fuzz run malformed_envelope -- -max_total_time=60
cargo +nightly fuzz run ipc_schema -- -max_total_time=60
cargo +nightly fuzz run version_downgrade -- -max_total_time=60
```

## Key Files for Cryptographic Review

| File | What to Check |
|------|--------------|
| `packages/crypto-core/src/aead.rs` | AES-256-GCM, nonce generation, AAD binding |
| `packages/crypto-core/src/kdf.rs` | Argon2id params, minimum policy |
| `packages/crypto-core/src/keys.rs` | Zeroization, constant-time eq |
| `packages/crypto-core/src/memlock.rs` | mlock/VirtualLock correctness |
| `packages/vault-runtime/src/hardening.rs` | Panic isolation, error sanitization |
| `packages/shared-types/src/ipc.rs` | HMAC-SHA256, replay counter |

## Key Files for Protocol Review

| File | What to Check |
|------|--------------|
| `apps/desktop/native-messaging-host/src/main.rs` | Origin validation, schema validation |
| `packages/vault-runtime/src/ipc.rs` | Session lifecycle, expiry, replay |
| `apps/extension/src/background/service-worker.ts` | Origin verification |
| `apps/extension/src/content/autofill-guard.ts` | Policy enforcement |
