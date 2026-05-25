# ESPASS Backend

Rust backend API for identity, device registration, encrypted vault synchronization, audit logging, and admin policy enforcement.

The backend stores encrypted blobs and metadata only. It must not receive master passwords, plaintext vault items, plaintext TOTP seeds, or usable vault keys.

