# ADR 0002: Zero-Knowledge Client-Side Encryption

## Status

Accepted

## Context

Enterprise customers need assurance that ESPASS operators, cloud administrators, and backend compromise scenarios cannot expose vault plaintext.

## Decision

Clients derive and use encryption keys locally. The backend stores encrypted blobs, metadata, device records, policy, and audit events only. Master passwords, plaintext vault items, plaintext TOTP seeds, and usable vault keys never leave client trust boundaries.

## Consequences

Server-side recovery of vault contents is impossible by design. Account recovery must use pre-planned client-side recovery keys, enterprise escrow with explicit policy, or hardware-backed recovery flows, each documented and visible to administrators and users.

