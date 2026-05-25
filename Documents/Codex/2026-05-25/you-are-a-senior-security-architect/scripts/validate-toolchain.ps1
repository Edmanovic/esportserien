$ErrorActionPreference = "Stop"

$required = @("rustup", "cargo", "rustc", "node", "npm")
foreach ($name in $required) {
  if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
    Write-Error "Missing required tool: $name"
  }
}

$link = Get-Command link.exe -ErrorAction SilentlyContinue
if (-not $link) {
  $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
  if (Test-Path $vswhere) {
    $installation = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($installation) {
      Write-Host "Visual Studio C++ tools found at $installation, but link.exe is not on PATH. Run from a Developer PowerShell or load VsDevCmd.bat."
    }
  }
  Write-Error "Missing required MSVC linker: link.exe. Install Visual Studio Build Tools with Microsoft.VisualStudio.Workload.VCTools."
}

$expectedRust = "1.95.0"
$rustcVersion = rustc --version
if ($rustcVersion -notmatch $expectedRust) {
  Write-Error "Rust compiler mismatch. Expected $expectedRust from rust-toolchain.toml, got: $rustcVersion"
}

$securityTools = @("cargo-audit", "cargo-deny", "cargo-fuzz", "cargo-nextest", "cargo-hakari", "cargo-udeps")
foreach ($tool in $securityTools) {
  $installed = cargo install --list | Select-String -Pattern "^$tool "
  if (-not $installed) {
    Write-Error "Missing required cargo security tool: $tool"
  }
}

Write-Host "Toolchain validation passed."
