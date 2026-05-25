# Compliance Readiness

## GDPR

- Data minimization: vault plaintext is never collected by the backend.
- Subject access: export metadata and encrypted vault blobs; decrypted export is client-side only.
- Deletion: support tenant, user, device, and vault deletion workflows.
- Retention: audit and operational logs have documented retention windows.

## SOC2

Initial controls:

- Change management through signed commits and reviewed pull requests.
- CI security checks for SAST, dependency audit, secret scanning, and tests.
- Access controls for production and audit log review.
- Incident response playbooks and tabletop exercises.

## ISO27001

Initial alignment:

- Asset inventory.
- Risk register.
- Secure development lifecycle.
- Supplier and dependency management.
- Business continuity and disaster recovery documentation.

