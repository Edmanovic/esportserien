# Runtime Hardening Report

## Scope

This report covers the Phase 3 secure runtime prototype:

- Local encrypted vault persistence.
- Unlock lifecycle and runtime secret store.
- Desktop-extension IPC handshake and replay protection.
- Secure autofill request path.
- Device trust runtime.
- Minimal encrypted sync backend.

## Security Invariants

| Invariant | Runtime Control |
| --- | --- |
| No plaintext vault data on disk | `VaultPersistenceEngine` writes AEAD ciphertext only |
| Local corruption is detected | AEAD tag plus local HMAC integrity tag |
| Vault writes are atomic | Same-directory create-new temp file, fsync, rename |
| Password-derived key lifetime is short | `UnlockManager` derives `MasterKey`, unwraps `VaultKey`, and does not return the master key |
| Runtime vault key can be locked | `RuntimeSecretStore` drops and zeroizes the key on lock |
| IPC messages fail closed | Origin pinning, protocol version validation, signed envelopes, counters |
| Autofill decrypts minimal scope | Origin policy passes before credential payload decryption |
| Backend cannot decrypt | Backend accepts only encrypted payload schemas |

## Residual Risks

- Memory locking is best-effort and can fail under operating system limits.
- A fully compromised endpoint can still read secrets while the vault is unlocked.
- Native messaging host registration must be installed by a trusted installer and pinned to release extension IDs.
- The prototype returns an IPC session-key confirmation over the native messaging channel; production should replace this with OS-mediated channel binding or asymmetric handshake encryption.

## Release Blockers

- Run `cargo test --workspace`, `cargo clippy`, `cargo audit`, `cargo deny`, and fuzz targets on a machine with Rust installed.
- Replace extension manifest placeholders with production extension IDs.
- Implement signed updater key material and custody.
- Add persistent device registry storage encrypted at rest.

