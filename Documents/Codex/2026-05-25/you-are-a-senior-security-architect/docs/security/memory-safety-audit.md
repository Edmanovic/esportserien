# Memory Safety and Secret Lifecycle Audit

## Secret Types

| Type | Clone | Debug | Drop Behavior | Memory Lock |
| --- | --- | --- | --- | --- |
| `SecureBuffer` | No | Redacted length only | Zeroize | Yes |
| `MasterKey` | No | Redacted | Zeroize | Yes |
| `VaultKey` | No | Redacted | Zeroize | Yes |
| `DeviceKey` | No | Redacted | Zeroize | Yes |
| `SessionKey` | No | Redacted | Zeroize | Yes |

## Lifecycle Map

1. User password enters a `SecureBuffer`.
2. `derive_master_key_from_buffer` derives a `MasterKey` and wipes the password buffer.
3. `UnlockManager` uses `MasterKey` only to unwrap the encrypted `VaultKey`.
4. `MasterKey` is not returned by the unlock flow.
5. `RuntimeSecretStore` holds `VaultKey` while unlocked and attempts memory locking.
6. Auto-lock drops the vault key, triggering zeroization.
7. Credential plaintext is decrypted into short-lived `SecureBuffer` and decoded only after origin policy passes.

## Panic Cleanup

Secret-owning types use `ZeroizeOnDrop`, so normal unwinding drops wipe buffers. Release profile uses `panic = "abort"` for reduced runtime complexity; abort cannot guarantee destructors for in-flight secrets, so crash dumps must be disabled or protected by platform policy before production release.

## Residual Exposure Risks

- OS memory locking may fail under quotas or sandboxing.
- Renderer compromise can observe data explicitly returned to it; privileged commands remain minimal.
- Clipboard contents are inherently exposed to OS-level observers until cleared.
- Miri was attempted, but Windows application-control policy blocked generated Miri build scripts.

