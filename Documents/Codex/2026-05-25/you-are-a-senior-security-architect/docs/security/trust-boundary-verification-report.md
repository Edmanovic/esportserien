# Trust Boundary Verification Report

## Verified Boundaries

| Boundary | Validation |
| --- | --- |
| Crypto envelope | Property tests reject mutation and downgrade |
| Local vault file | Runtime test rejects tampered revision/integrity tag |
| Extension IPC | Runtime test rejects replayed signed envelope |
| Autofill origin | Runtime test blocks cross-origin iframe before decryption |
| Session runtime | Runtime test expires and wipes unlocked vault key |
| Backend storage | Backend upload validates encrypted payload envelope shape only |

## Commands

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit
cargo deny check
npm run security-lab
```

## Notes

These commands were executed on Windows with Visual Studio Build Tools loaded through `VsDevCmd.bat`.

