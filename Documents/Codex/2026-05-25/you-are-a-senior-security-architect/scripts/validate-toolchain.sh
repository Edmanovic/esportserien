#!/usr/bin/env bash
set -euo pipefail

for tool in rustup cargo rustc node npm; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "Missing required tool: $tool" >&2
    exit 1
  }
done

expected_rust="1.95.0"
if ! rustc --version | grep -q "$expected_rust"; then
  echo "Rust compiler mismatch. Expected $expected_rust from rust-toolchain.toml; got $(rustc --version)" >&2
  exit 1
fi

for cargo_tool in cargo-audit cargo-deny cargo-fuzz cargo-nextest cargo-hakari cargo-udeps; do
  cargo install --list | grep -q "^${cargo_tool} " || {
    echo "Missing required cargo security tool: ${cargo_tool}" >&2
    exit 1
  }
done

echo "Toolchain validation passed."
