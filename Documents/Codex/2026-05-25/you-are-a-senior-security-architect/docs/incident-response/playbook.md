# Incident Response Playbook

## Severity Triggers

- Confirmed plaintext secret exposure.
- Authentication bypass.
- Backend database or object storage compromise.
- Signing key compromise.
- Malicious release or extension update.

## First Hour

1. Assign incident commander.
2. Freeze relevant release channels.
3. Preserve logs and affected artifacts.
4. Revoke exposed sessions or signing credentials when confirmed.
5. Start customer impact analysis.

## Zero-Knowledge Breach Note

If encrypted blobs are exfiltrated without client keys or master passwords, impact is limited but still serious. Response must include forced session rotation, KDF parameter review, password strength guidance, and transparent customer communication.

