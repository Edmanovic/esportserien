# ADR 0001: Use Rust for Security-Sensitive Core

## Status

Accepted

## Context

ESPASS must handle password-derived keys, vault encryption, synchronization correctness, and backend authorization boundaries. Memory safety and type-level correctness are priority requirements.

## Decision

Use Rust for `crypto-core`, backend services, sync-critical code, and Tauri native commands.

## Consequences

Rust reduces broad classes of memory safety vulnerabilities and provides strong tooling for tests, fuzzing, and dependency auditing. It increases implementation complexity for frontend-focused contributors, so narrow TypeScript bindings and documentation are required.

