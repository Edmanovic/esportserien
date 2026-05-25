# Residual Risk Registry

| Risk | Impact | Current Mitigation | Status |
| --- | --- | --- | --- |
| Endpoint compromise while vault unlocked | Credential disclosure | Auto-lock, minimal plaintext lifetime, zeroize | Accepted operational risk |
| Memory locking unavailable | Swap/pagefile exposure risk | Best-effort lock, documented failure mode | Needs platform QA |
| Native messaging installation tampering | Extension/Desktop impersonation | Origin pinning, signed releases planned | Release blocker |
| Clipboard interception | Credential disclosure | TTL clear, user action requirement | Needs OS-specific hardening |
| IPC session key exposure over native channel | Request forgery | Short TTL, native channel locality | Improve before external beta |

