//! Shared ESPASS schema and protocol types.
//!
//! These types model encrypted boundaries only. Backend-compatible structures
//! intentionally do not contain vault plaintext, master passwords, or usable
//! encryption keys.

pub mod autofill;
pub mod device;
pub mod ipc;
pub mod session;
pub mod vault;
