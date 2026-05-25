use core::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{CryptoError, MemoryLock, MemoryLockGuard};

/// A non-clonable sensitive byte buffer that zeroizes on drop.
#[derive(Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct SecureBuffer(Vec<u8>);

impl SecureBuffer {
    /// Creates a sensitive buffer from owned bytes.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Creates a zero-filled sensitive buffer.
    #[must_use]
    pub fn zeroed(len: usize) -> Self {
        Self(vec![0_u8; len])
    }

    /// Returns the number of bytes in the buffer.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true when the buffer contains no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Borrows sensitive bytes for cryptographic operations.
    #[must_use]
    pub fn expose_secret(&self) -> &[u8] {
        &self.0
    }

    /// Mutably borrows sensitive bytes for cryptographic operations.
    #[must_use]
    pub fn expose_secret_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }

    /// Consumes the buffer and returns the inner bytes.
    #[must_use]
    pub fn into_inner(mut self) -> Vec<u8> {
        core::mem::take(&mut self.0)
    }
}

impl fmt::Debug for SecureBuffer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecureBuffer")
            .field("len", &self.0.len())
            .field("redacted", &true)
            .finish()
    }
}

impl MemoryLock for SecureBuffer {
    fn lock_memory(&mut self) -> Result<MemoryLockGuard, CryptoError> {
        MemoryLockGuard::new(self.0.as_mut_ptr(), self.0.len())
    }
}
