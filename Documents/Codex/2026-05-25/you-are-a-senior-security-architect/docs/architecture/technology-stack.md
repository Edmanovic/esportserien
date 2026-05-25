# Technology Stack Decisions

## Selected Stack

| Area | Choice | Reason |
| --- | --- | --- |
| Crypto core | Rust | Memory safety, strong type system, mature crypto ecosystem |
| Desktop | Tauri + React + TypeScript | Smaller attack surface than bundled Chromium-only shells, Rust backend commands |
| Extension | TypeScript WebExtension | Chromium and Firefox support with strict typing |
| Backend | Rust + Axum | High-performance async server, strong correctness tooling |
| Database | PostgreSQL | Reliable relational metadata, mature operational tooling |
| Policy | Open Policy Agent | Auditable enterprise authorization policies |
| Auth | WebAuthn/FIDO2 plus secure sessions | Phishing-resistant enterprise authentication |
| Infrastructure | Docker, Kubernetes, Terraform | Portable local dev and enterprise deployment |
| CI/CD | GitHub Actions | Broad ecosystem for SAST, dependency, secret, and release workflows |

## Tradeoffs

Tauri introduces native platform complexity, but it gives ESPASS better control over OS secure storage, native messaging, process hardening, and binary signing. Rust increases onboarding cost but is appropriate for cryptography, sync correctness, and backend security boundaries.

AES-256-GCM is selected for the MVP because it is standardized, fast, audited, and widely supported. libsodium remains planned for public-key sharing and sealed boxes where its high-level APIs reduce implementation risk.

