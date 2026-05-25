#![no_main]

use espass_crypto_core::{decrypt, EncryptedEnvelope, VaultKey};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 13 {
        return;
    }

    let mut nonce = [0_u8; 12];
    nonce.copy_from_slice(&data[1..13]);
    let envelope = EncryptedEnvelope {
        version: data[0],
        nonce,
        ciphertext: data[13..].to_vec(),
    };
    let key = VaultKey::from_bytes([9_u8; 32]);
    let _ = decrypt(&key, &envelope, b"fuzz:aad");
});

