# Reproducible Build System

## Toolchain Pinning

ESPASS pins Rust through `rust-toolchain.toml` and disables incremental builds in `.cargo/config.toml`. The Phase 4 baseline is Rust 1.95.0 because the 2026 dependency graph includes Edition 2024 metadata in transitive crates. Release builds use a single codegen unit, ThinLTO, stripped symbols, and abort-on-panic to reduce nondeterministic build output and runtime attack surface.

## Bootstrap

Use:

```powershell
.\scripts\bootstrap.ps1
.\scripts\validate-toolchain.ps1
```

or:

```bash
./scripts/bootstrap.sh
./scripts/validate-toolchain.sh
```

The bootstrap scripts validate Rust, Node, npm, and required security tools: `cargo-audit`, `cargo-deny`, and `cargo-fuzz`.

## Artifact Signing

Desktop, extension, and backend artifacts must be signed before production distribution. The current repository contains updater placeholders only; production release remains blocked until signing keys and key custody procedures are documented.

## SBOM and Integrity

CI generates an SPDX SBOM and runs dependency scanners. Release promotion requires matching SBOM, source revision, signed artifact digest, and dependency audit results.
