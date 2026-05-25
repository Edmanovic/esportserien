# ESPASS Residual Risk Summary

| ID | Risk | Likelihood | Impact | Residual Mitigation | Owner |
|----|------|-----------|--------|-------------------|-------|
| RR-01 | Crash dump contains decrypted vault key | Low | Critical | mlock reduces likelihood; OS crash dump exclusion TBD | Engineering |
| RR-02 | Swap file exposure of plaintext | Low | High | mlock pins pages; encrypted swap (OS setting) recommended to users | Documentation |
| RR-03 | Timing oracle via Argon2id unlock latency | Low | Medium | Exponential backoff throttles; timing variation is inherent to KDF | Accepted |
| RR-04 | Extension isolated world breakout | Very Low | Critical | Depends on browser security; mitigated by MV3 restrictions | Accepted |
| RR-05 | Native messaging binary substitution | Low | Critical | Sigstore signing; system integrity (Gatekeeper/Windows Defender) | DevOps |
| RR-06 | Clipboard sniffing between copy and clear | Low | High | 30s TTL; users advised to use autofill instead of copy | Documentation |
| RR-07 | Session key in memory after IPC session expires | Low | Medium | IpcSessionRegistry evicts expired sessions on next validate call; proactive eviction TBD | Engineering |
