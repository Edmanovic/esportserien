# Shared Types

Cross-application TypeScript and Rust-compatible schemas for identifiers, encrypted payload metadata, audit-safe DTOs, and API contracts.

Rules:

- Do not define plaintext vault item models for backend use.
- Prefer generated schemas from a single source of truth.
- Mark sensitive fields explicitly and keep redaction behavior testable.

