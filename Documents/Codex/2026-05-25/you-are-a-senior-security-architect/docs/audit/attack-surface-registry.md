# ESPASS Attack Surface Registry

## Attack Surface Entry Points

| ID | Surface | Protocol | Auth Required | Mitigations |
|----|---------|----------|---------------|-------------|
| AS-01 | Native messaging stdin | JSON/length-prefix | Extension origin pinning + handshake | Max message size, schema validation |
| AS-02 | Tauri IPC | Internal WebView | Renderer same-origin | Command allowlist, panic isolation |
| AS-03 | Extension content script | DOM events | User gesture | Visibility, overlay, punycode, cross-origin checks |
| AS-04 | Backend HTTP API | HTTPS | (TBD — bearer token) | Rate limiting, anomaly detection, payload size limits |
| AS-05 | Local vault file | Filesystem | OS file permissions | HMAC integrity tag, encrypted at rest |
| AS-06 | Clipboard | OS clipboard | — | 30-second TTL clear |
| AS-07 | Fuzz surface (crypto) | Direct API | — | Version validation, payload size limits, AAD binding |

## Threat Actor Capabilities Assumed

- **Malicious web page:** Can control page DOM, inject scripts (within CSP), attempt phishing
- **Compromised renderer:** Can call any allowlisted Tauri command with arbitrary arguments
- **Compromised extension:** Can attempt malformed IPC messages; cannot bypass HMAC without session key
- **Network attacker:** Can observe encrypted sync traffic; cannot decrypt without vault key
- **Physical attacker:** Can read swap/crash dumps; mitigated by mlock and zeroize

## Not Mitigated (Residual)

- Full OS compromise — out of scope for a client-side password manager
- Hardware-level memory attacks (cold boot, DMA) — mitigated only by mlock, not eliminated
- Browser zero-days that break the extension isolated world
