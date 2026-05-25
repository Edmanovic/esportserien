# ESPASS Security Lab Report

Generated: 2026-05-25T15:38:39.475Z

| Attack | Severity | Expected Result | Status |
| --- | --- | --- | --- |
| ipc-replay | high | desktop rejects the message with IPC replay detected | designed |
| malformed-handshake | high | native messaging host rejects handshake before session creation | designed |
| origin-spoofing | critical | extension blocks autofill before contacting native host | designed |
| session-expiry-race | high | runtime fails closed and wipes unlocked vault key before serving request | designed |
| vault-corruption | critical | vault persistence engine refuses to decrypt and reports integrity failure | designed |
| vault-tampering | critical | crypto-core rejects decryption with authentication failure | designed |

## Mitigations

- ipc-replay: exploitability 7/10; mitigation: monotonic per-session counters and short IPC session TTL
- malformed-handshake: exploitability 6/10; mitigation: strict schema validation, origin pinning, protocol version checks
- origin-spoofing: exploitability 9/10; mitigation: exact origin matching, iframe blocking, punycode suspicion, user gesture gating
- session-expiry-race: exploitability 5/10; mitigation: central auto-lock enforcement and runtime secret store lock operation
- vault-corruption: exploitability 8/10; mitigation: atomic writes, AEAD, local keyed integrity tag, schema version validation
- vault-tampering: exploitability 8/10; mitigation: AEAD tags plus local HMAC integrity tags over deterministic metadata
