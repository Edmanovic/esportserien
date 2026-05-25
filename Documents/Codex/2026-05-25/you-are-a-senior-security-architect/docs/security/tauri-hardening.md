# Tauri Runtime Hardening

## Baseline

- Strict CSP: no `unsafe-eval`, no remote scripts, no object embedding, no framing.
- Capability allowlist starts with `core:default` only.
- Rust commands are added one-by-one after threat analysis and schema validation.
- Filesystem access is isolated to app-owned directories and encrypted cache paths.
- Updater must use signed artifacts before public release.
- Dev URLs are development-only and must not be used in production bundles.

## Attack Surface Review

| Surface | Risk | Control |
| --- | --- | --- |
| Tauri command IPC | Unvalidated frontend request reaches Rust | Typed commands, explicit capabilities, authz checks |
| Frontend CSP | Script injection | No eval, no inline scripts, local assets only |
| Updater | Malicious update | Signed artifacts, pinned update metadata |
| Plugins | Excess native privilege | Minimal plugin set and ADR for each plugin |
| Local files | Vault cache exposure | Encrypted cache, scoped paths, no broad FS permission |

## Release Gate

Production release is blocked until signing keys, updater public key, platform notarization/signing, and reproducible build notes are complete.

