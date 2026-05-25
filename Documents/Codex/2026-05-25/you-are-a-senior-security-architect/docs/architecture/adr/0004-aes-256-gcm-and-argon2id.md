# ADR 0004: Use AES-256-GCM and Argon2id for MVP Cryptography

## Status

Accepted

## Context

The MVP needs well-reviewed primitives for password-based key derivation and authenticated encryption.

## Decision

Use Argon2id for password-based key derivation and AES-256-GCM for authenticated encryption. Use OS CSPRNG-generated salts and nonces. Use associated data for contextual binding.

## Consequences

This gives a conservative baseline with broad platform support. Nonce uniqueness is mandatory and enforced by generating a fresh nonce for every encryption operation.

