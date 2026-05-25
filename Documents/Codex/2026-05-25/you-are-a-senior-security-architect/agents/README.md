# ESPASS Internal Security Agent Framework

Internal agents are automated review workflows that run locally and in CI. They produce findings and remediation tasks; they do not silently change security-sensitive code.

## Agents

| Agent | Responsibilities |
| --- | --- |
| Security Agent | Vulnerability scanning, crypto usage checks, insecure pattern detection, OWASP validation |
| Red Team Agent | Abuse-case tests, API fuzzing, phishing resistance checks, session attack simulations |
| Architecture Agent | Boundary enforcement, dependency direction, modularity and scalability review |
| Compliance Agent | GDPR, SOC2, ISO27001 evidence and retention policy checks |
| QA Agent | Integration, E2E, regression, UI, and performance testing |

## Finding Format

```json
{
  "agent": "security-agent",
  "severity": "high",
  "title": "Plaintext secret logged",
  "file": "apps/backend/src/main.rs",
  "line": 42,
  "recommendation": "Use redacted logging wrapper"
}
```

