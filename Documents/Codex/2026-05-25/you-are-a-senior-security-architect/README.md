# ESPASS

ESPASS is a production-grade enterprise password manager designed around a zero-knowledge security model. The server stores encrypted vault material and operational metadata only; vault plaintext, master passwords, and data-encryption keys never leave trusted client contexts.

## Current State

This repository is being built in phases. The first implemented package is `packages/crypto-core`, a Rust crate that provides audited primitives for local key derivation and vault item encryption.

## Repository Layout

```text
apps/
  desktop/          Tauri desktop application, first priority
  extension/        Chromium/Firefox WebExtension, first priority
  backend/          Secure synchronization and admin API
  admin/            Enterprise admin console
  future-mobile/    iOS/Android architecture notes and future app shells
packages/
  crypto-core/      Rust cryptographic core
  shared-types/     Cross-app API and domain types
  auth/             Client and server auth helpers
  ui/               Shared React UI primitives
  security/         Security policy checks and secure coding utilities
  audit/            Audit event schemas and emitters
  sync-engine/      Offline-first encrypted vault sync
infrastructure/
  docker/           Local and production container setup
  k8s/              Kubernetes manifests
  terraform/        Cloud infrastructure as code
  monitoring/       Logs, metrics, alerts
docs/
  architecture/     System design, diagrams, ADRs
  threat-model/     STRIDE and abuse-case analysis
  security/         Cryptography and secure coding guidance
  compliance/       GDPR, SOC2, ISO27001 mapping
  api/              Backend API design
  incident-response/ Playbooks and response process
agents/             Internal review agent framework
```

## Security Baseline

- Zero-knowledge by default.
- Client-side encryption before synchronization.
- Argon2id for password-based key derivation.
- AES-256-GCM for authenticated encryption.
- Random 96-bit nonces per encryption operation.
- No custom cryptography.
- No plaintext secrets in storage, logs, telemetry, or server responses.

## Local Development

```powershell
cargo test --workspace
```
