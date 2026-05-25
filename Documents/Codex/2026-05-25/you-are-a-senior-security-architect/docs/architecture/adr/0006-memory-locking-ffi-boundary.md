# ADR 0006: Contain Memory Locking FFI in Crypto Core

## Status

Accepted

## Context

ESPASS should reduce exposure of master keys, vault keys, and decrypted buffers to swapping where the operating system allows it. Rust does not provide cross-platform memory locking in the standard library.

## Decision

Implement a small memory locking abstraction in `packages/crypto-core` using `mlock` on Unix platforms and `VirtualLock` on Windows. This is the only approved unsafe Rust boundary in the current codebase. The unsafe calls are limited to live Rust-owned buffers and return a guard that unlocks memory on drop.

## Consequences

Memory locking is best-effort and may fail under OS limits or sandbox policies. Callers must treat failure as a hardening loss, not as proof that secrets are safe elsewhere. The rest of the workspace keeps unsafe Rust forbidden.

