# End-to-End Secure Runtime Flow

```mermaid
sequenceDiagram
  participant U as User
  participant D as Desktop Runtime
  participant V as Local Vault Store
  participant H as Native Messaging Host
  participant E as Extension Background
  participant C as Content Script
  participant B as Backend Sync API

  U->>D: Enter master password
  D->>D: Argon2id derive MasterKey
  D->>D: Decrypt VaultKey key slot
  D->>V: Load encrypted local vault
  V-->>D: Ciphertext and integrity tag
  D->>D: Validate HMAC and AEAD
  C->>E: Origin-validated autofill request
  E->>H: Extension handshake
  H->>H: Pin origin and create IPC session
  E->>H: Signed credential request
  H->>D: Broker request
  D->>D: Validate session, device, origin, field policy
  D->>D: Decrypt minimal credential scope
  D-->>E: Signed response for immediate fill
  E-->>C: Inject into validated field
  D->>B: Upload/download encrypted blobs only
```

## Failure Modes

- Any schema parse failure is denied.
- Any IPC counter replay is denied.
- Any origin mismatch is denied before decryption.
- Any local vault integrity failure prevents unlock.
- Any expired session locks runtime secrets before serving requests.

