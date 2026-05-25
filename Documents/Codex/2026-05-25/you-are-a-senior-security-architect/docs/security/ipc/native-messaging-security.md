# Native Messaging and IPC Security

## Trust Boundary

The browser extension and desktop app are separate principals. Web page content is untrusted. Content scripts may inspect DOM fields but must not retain vault secrets. The desktop vault service is the only component allowed to request decrypted credentials from an unlocked vault.

## Protocol

```mermaid
sequenceDiagram
  participant C as Content Script
  participant B as Extension Background
  participant H as Native Messaging Host
  participant D as Desktop Vault Service
  C->>B: Credential request with origin evidence
  B->>H: Handshake with extension origin and nonce
  H->>B: Handshake accepted
  B->>H: Signed IPC envelope with counter
  H->>D: Validated request
  D->>H: Redacted response or secret for immediate fill
  H->>B: Signed response
```

## Controls

- Native messaging host pins exact extension origins. Chrome does not allow wildcards in `allowed_origins`, and the host also verifies the caller origin passed by the browser.
- Messages are length-prefixed JSON per browser native messaging protocol and capped before parsing.
- Signed IPC envelopes include session ID, correlation ID, counter, timestamp, and payload.
- Replay protection requires monotonically increasing counters per IPC session.
- Extension content scripts never store secrets and never receive vault keys.
- Autofill requires exact origin match, visible fields, no cross-origin iframe, and user gesture when policy requires it.

## References Checked

- Chrome native messaging documentation notes stdio framing, extension caller origin, and message size limits.
- Chrome Manifest V3 documentation requires packaged extension code and service-worker background contexts.
- Mozilla WebExtensions documentation describes native messaging host manifests and the `nativeMessaging` permission.
- Tauri capabilities documentation describes constraining frontend exposure through per-window permissions.

