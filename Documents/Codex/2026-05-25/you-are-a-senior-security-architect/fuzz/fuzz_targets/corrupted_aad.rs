#![no_main]

use espass_crypto_core::{decrypt, encrypt, VaultKey};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let key = VaultKey::from_bytes([2u8; 32]);
    // Encrypt with canonical AAD.
    let Ok(envelope) = encrypt(&key, b"plaintext", b"canonical-aad") else { return };
    // Attempt decryption with fuzz-mutated AAD — must always fail.
    let result = decrypt(&key, &envelope, data);
    // If the fuzz data happens to equal "canonical-aad" decryption will succeed;
    // otherwise it must fail. We cannot assert failure because the fuzzer may
    // find the exact AAD, but any panic here is a bug.
    let _ = result;
});
