#![no_main]

use espass_crypto_core::{decrypt, EncryptedEnvelope, VaultKey};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 13 {
        return;
    }
    // Force version byte to an invalid value (anything but 1) to test downgrade rejection.
    let version = data[0].wrapping_add(2);
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&data[1..13]);
    let envelope = EncryptedEnvelope {
        version,
        nonce,
        ciphertext: data[13..].to_vec(),
    };
    let key = VaultKey::from_bytes([1u8; 32]);
    // Must always return Err — version 0 and versions >1 are unsupported.
    let result = decrypt(&key, &envelope, b"fuzz:aad");
    if version != 1 {
        assert!(result.is_err(), "version {version} should be rejected");
    }
});
