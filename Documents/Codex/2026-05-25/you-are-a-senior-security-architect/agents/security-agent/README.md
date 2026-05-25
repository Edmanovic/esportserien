# Security Agent

Runs:

- Secret scanning.
- Dependency audit.
- Static insecure pattern checks.
- Crypto misuse checks.
- OWASP ASVS checklist validation.

Initial commands:

```powershell
cargo audit
cargo clippy --workspace --all-targets -- -D warnings
```

