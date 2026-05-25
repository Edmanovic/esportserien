use rand_core::{OsRng, RngCore};

use crate::CryptoError;

/// Fills a caller-provided buffer with bytes from the operating system CSPRNG.
pub fn fill_random(bytes: &mut [u8]) -> Result<(), CryptoError> {
    OsRng
        .try_fill_bytes(bytes)
        .map_err(|_| CryptoError::RandomFailed)
}

/// Returns a fixed-size random byte array.
pub fn random_array<const N: usize>() -> Result<[u8; N], CryptoError> {
    let mut bytes = [0_u8; N];
    fill_random(&mut bytes)?;
    Ok(bytes)
}

/// Returns a random byte vector of the requested length.
pub fn random_vec(len: usize) -> Result<Vec<u8>, CryptoError> {
    let mut bytes = vec![0_u8; len];
    fill_random(&mut bytes)?;
    Ok(bytes)
}
