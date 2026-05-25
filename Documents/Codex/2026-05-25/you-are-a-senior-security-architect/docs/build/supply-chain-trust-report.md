# Supply-Chain Trust Report

## Current Controls

- Pinned Rust toolchain: 1.95.0.
- Deterministic workspace lockfile: `Cargo.lock`.
- `cargo-audit` vulnerability scan passes.
- `cargo-deny` advisory, license, source, and banned-crate policy passes.
- CI includes Semgrep, Gitleaks, OSV scanner, Trivy, SBOM generation, nextest, Hakari, udeps, and Miri gates.

## Current Warnings

`cargo-deny` reports duplicate versions of `getrandom`, `windows-sys`, and `wit-bindgen`. These are warnings, not failures, and are driven by ecosystem transitions across crypto/runtime/test dependencies. They should be revisited before beta.

## Release Signing Status

Release signing is designed but not active. Production release remains blocked until:

- Desktop signing certificates are provisioned.
- Extension release IDs are finalized.
- Updater public key is configured.
- Artifact provenance attestations are generated.
- Signing-key custody and rotation are documented.

