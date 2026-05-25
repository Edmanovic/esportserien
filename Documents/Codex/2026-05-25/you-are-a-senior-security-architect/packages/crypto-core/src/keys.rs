use core::fmt;

use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{CryptoError, MemoryLock, MemoryLockGuard};

const KEY_LEN: usize = 32;

macro_rules! secret_key_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Zeroize, ZeroizeOnDrop)]
        pub struct $name([u8; KEY_LEN]);

        impl $name {
            /// Creates the key from exactly 32 bytes.
            #[must_use]
            pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
                Self(bytes)
            }

            /// Borrows key material for cryptographic operations.
            #[must_use]
            pub fn expose_secret(&self) -> &[u8; KEY_LEN] {
                &self.0
            }

            /// Constant-time equality for key confirmation checks.
            #[must_use]
            pub fn ct_eq(&self, other: &Self) -> bool {
                self.0.ct_eq(&other.0).into()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_struct(stringify!($name))
                    .field("len", &KEY_LEN)
                    .field("redacted", &true)
                    .finish()
            }
        }

        impl MemoryLock for $name {
            fn lock_memory(&mut self) -> Result<MemoryLockGuard, CryptoError> {
                MemoryLockGuard::new(self.0.as_mut_ptr(), self.0.len())
            }
        }
    };
}

secret_key_type!(MasterKey, "Password-derived key-encryption key.");
secret_key_type!(VaultKey, "Vault data-encryption key.");
secret_key_type!(DeviceKey, "Symmetric device-local protection key.");
secret_key_type!(SessionKey, "Ephemeral IPC or local session key.");
