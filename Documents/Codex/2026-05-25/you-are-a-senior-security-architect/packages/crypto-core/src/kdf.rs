use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::{random_array, CryptoError, MasterKey, SecureBuffer};

const KEY_LEN: usize = 32;
const SALT_LEN: usize = 16;
const MIN_MEMORY_COST_KIB: u32 = 64 * 1024;
const MIN_ITERATIONS: u32 = 3;
const MIN_PARALLELISM: u32 = 1;

/// Argon2id parameters used to derive a 256-bit ESPASS key.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct KdfParams {
    /// Argon2 memory cost in KiB.
    pub memory_cost_kib: u32,
    /// Argon2 iteration count.
    pub iterations: u32,
    /// Argon2 lane count.
    pub parallelism: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            memory_cost_kib: 194_560,
            iterations: 3,
            parallelism: 1,
        }
    }
}

impl KdfParams {
    /// Validates the parameter set against ESPASS minimum security policy.
    #[must_use]
    pub fn meets_minimum_policy(&self) -> bool {
        self.memory_cost_kib >= MIN_MEMORY_COST_KIB
            && self.iterations >= MIN_ITERATIONS
            && self.parallelism >= MIN_PARALLELISM
    }
}

/// A random salt for Argon2id key derivation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Salt([u8; SALT_LEN]);

impl Salt {
    /// Generates a fresh random salt.
    pub fn generate() -> Result<Self, CryptoError> {
        Ok(Self(random_array()?))
    }

    /// Constructs a salt from a fixed byte array.
    #[must_use]
    pub fn from_bytes(bytes: [u8; SALT_LEN]) -> Self {
        Self(bytes)
    }

    /// Returns the salt bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; SALT_LEN] {
        &self.0
    }
}

/// Derives a master key from a master password and salt using Argon2id.
pub fn derive_master_key(
    password: &[u8],
    salt: &Salt,
    params: KdfParams,
) -> Result<MasterKey, CryptoError> {
    if !params.meets_minimum_policy() {
        return Err(CryptoError::InvalidKdfParameters);
    }

    let argon_params = Params::new(
        params.memory_cost_kib,
        params.iterations,
        params.parallelism,
        Some(KEY_LEN),
    )
    .map_err(|_| CryptoError::InvalidKdfParameters)?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut key = [0_u8; KEY_LEN];
    argon2
        .hash_password_into(password, salt.as_bytes(), &mut key)
        .map_err(|_| CryptoError::InvalidKdfParameters)?;

    Ok(MasterKey::from_bytes(key))
}

/// Derives a master key from a zeroizing password buffer and wipes the password.
pub fn derive_master_key_from_buffer(
    password: &mut SecureBuffer,
    salt: &Salt,
    params: KdfParams,
) -> Result<MasterKey, CryptoError> {
    let key = derive_master_key(password.expose_secret(), salt, params);
    password.expose_secret_mut().zeroize();
    key
}
