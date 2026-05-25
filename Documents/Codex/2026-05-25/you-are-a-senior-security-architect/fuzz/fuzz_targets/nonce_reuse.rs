#![no_main]

use espass_crypto_core::{decrypt, EncryptedEnvelope, VaultKey};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 24 {
        return;
    }
    let key = VaultKey::from_bytes([3u8; 32]);
    // Two envelopes sharing the same nonce (nonce reuse).
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&data[0..12]);
    let envelope_a = EncryptedEnvelope {
        version: 1,
        nonce,
        ciphertext: data[12..].to_vec(),
    };
    let envelope_b = EncryptedEnvelope {
        version: 1,
        nonce,
        ciphertext: data[12..].to_vec(),
    };
    // Both decrypt calls must not panic; failure is expected and acceptable.
    let _ = decrypt(&key, &envelope_a, b"aad-a");
    let _ = decrypt(&key, &envelope_b, b"aad-b");
});
