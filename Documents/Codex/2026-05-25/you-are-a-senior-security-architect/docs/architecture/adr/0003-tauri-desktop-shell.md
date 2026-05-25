# ADR 0003: Use Tauri for Desktop

## Status

Accepted

## Context

ESPASS needs Windows, macOS, and Linux support with strong native integration and a smaller runtime footprint.

## Decision

Use Tauri with a React + TypeScript UI and Rust command backend.

## Consequences

Tauri provides native secure storage integration and a Rust execution boundary for sensitive operations. It requires careful IPC command validation and platform-specific signing and update pipelines.

