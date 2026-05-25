# Runtime Attack Surface Inventory

| Surface | Entry Point | Primary Threat | Current Control |
| --- | --- | --- | --- |
| Local vault file | Filesystem | Tampering, rollback, corruption | AEAD, HMAC tag, schema version checks, atomic writes |
| Unlock password | Desktop input | Key theft, brute force | Argon2id, zeroizing password buffer, throttling |
| Runtime vault key | Process memory | Memory scraping, swap | Zeroize, auto-lock, best-effort memory lock |
| Native messaging stdin | Browser host pipe | Malformed JSON, oversized message | Length cap, strict JSON schema |
| Extension handshake | Native host | Impersonation | Exact origin pinning, protocol version checks |
| IPC request | Native host | Replay, tampering | HMAC signed envelopes, counters, TTL |
| Content script | Web page DOM | DOM abuse, hidden fields | Visibility checks, iframe blocking, user gesture |
| Backend API | HTTP | Plaintext upload, oversized blobs | Encrypted payload schema, size/version rejection |
| Tauri command IPC | WebView bridge | Native command abuse | Minimal capability file, no vault commands exposed yet |

