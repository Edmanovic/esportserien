#!/usr/bin/env bash
set -euo pipefail

install_rust="${INSTALL_RUST:-0}"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

if [[ "$install_rust" == "1" ]] && ! command -v rustup >/dev/null 2>&1; then
  echo "Installing rustup. Review https://rustup.rs before running this in production images."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env"
fi

require_command rustup
require_command cargo
require_command rustc
require_command node
require_command npm

rustup show
rustup toolchain install 1.95.0 --profile minimal --component rustfmt --component clippy
cargo --version
rustc --version
node --version
npm --version

cargo install cargo-audit --locked
cargo install cargo-deny --locked
cargo install cargo-fuzz --locked
cargo install cargo-nextest --locked
cargo install cargo-hakari --locked
cargo install cargo-udeps --locked

echo "ESPASS bootstrap complete."
