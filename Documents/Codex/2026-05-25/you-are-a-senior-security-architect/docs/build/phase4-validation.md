# Phase 4 Validation

## Executed Locally

- `rustup toolchain install 1.95.0 --profile minimal --component rustfmt --component clippy`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo audit`
- `cargo deny check`
- `npm run security-lab`

On Windows, Cargo must run inside the Visual Studio Build Tools developer environment so `link.exe` is on `PATH`.

## Added Gates

- Property-based crypto tests with `proptest`.
- Security-lab exploitability scoring.
- CI wiring for `cargo-nextest`, `cargo-hakari`, `cargo-udeps`, and Miri.
- Toolchain validation for Rust 1.95.0 and Visual Studio C++ linker presence.

## Sanitizer Plan

Rust sanitizer runs require nightly and platform support. The intended command for Linux CI is:

```bash
RUSTFLAGS="-Z sanitizer=address" cargo +nightly test -Z build-std --target x86_64-unknown-linux-gnu --workspace
```

This remains a CI hardening target rather than a Windows local default.

## Miri Result

`cargo +nightly miri test -p espass-crypto-core --lib` was attempted. Nightly and Miri installed, but Windows application-control policy blocked generated build-script executables under `target/miri` with OS error 4551. This is an environment policy blocker, not a code failure.
