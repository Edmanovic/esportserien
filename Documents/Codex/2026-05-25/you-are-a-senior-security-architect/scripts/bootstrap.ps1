param(
  [switch]$InstallRust
)

$ErrorActionPreference = "Stop"

function Require-Command {
  param([string]$Name)
  $command = Get-Command $Name -ErrorAction SilentlyContinue
  if (-not $command) {
    throw "Missing required command: $Name"
  }
  return $command.Source
}

if ($InstallRust -and -not (Get-Command rustup -ErrorAction SilentlyContinue)) {
  Write-Host "Installing rustup. Review https://rustup.rs before running this in production images."
  winget install --id Rustlang.Rustup -e --source winget
  $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
  if (Test-Path $cargoBin) {
    $env:PATH = "$cargoBin;$env:PATH"
  }
}

Require-Command rustup | Out-Null
Require-Command cargo | Out-Null
Require-Command rustc | Out-Null
Require-Command node | Out-Null
Require-Command npm | Out-Null

rustup show
rustup toolchain install 1.95.0 --profile minimal --component rustfmt --component clippy
cargo --version
rustc --version
node --version
npm --version

if (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
  Write-Warning "MSVC link.exe not found. Install Visual Studio Build Tools with the C++ workload or run this from a Developer PowerShell."
}

cargo install cargo-audit --locked
cargo install cargo-deny --locked
cargo install cargo-fuzz --locked
cargo install cargo-nextest --locked
cargo install cargo-hakari --locked
cargo install cargo-udeps --locked

Write-Host "ESPASS bootstrap complete."
