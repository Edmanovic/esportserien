# Runtime Capability Matrix

| Component | May Hold Master Password | May Hold Master Key | May Hold Vault Key | May Hold Plain Credential | Persistent Storage |
| --- | --- | --- | --- | --- | --- |
| Backend | No | No | No | No | Encrypted blobs and metadata only |
| Desktop unlock manager | Briefly as zeroizing buffer | Briefly during unwrap | Yes while unlocked | Briefly per user action | Encrypted local vault only |
| Native messaging host | No | No | No | No by default; brokered only | None |
| Extension background | No | No | No | Briefly for immediate fill | No secret storage |
| Content script | No | No | No | Only field injection moment | None |
| Web page | No | No | No | Receives filled credential only after policy allows | Page-controlled |

