# Kubernetes

Production deployment target for ESPASS services.

Baseline controls:

- Separate namespaces for API, policy, monitoring, and jobs.
- Network policies.
- Pod security standards.
- Read-only root filesystems where practical.
- External secrets backed by cloud KMS.

