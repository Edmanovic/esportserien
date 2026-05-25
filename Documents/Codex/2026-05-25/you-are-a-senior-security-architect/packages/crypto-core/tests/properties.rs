//! Property-based cryptographic invariant tests.

use espass_crypto_core::{decrypt, encrypt, EncryptedEnvelope, VaultKey};
use proptest::prelude::*;

proptest! {
    #[test]
    fn aead_round_trips_arbitrary_payloads(payload in proptest::collection::vec(any::<u8>(), 0..4096), aad in proptest::collection::vec(any::<u8>(), 0..512)) {
        let key = VaultKey::from_bytes([42_u8; 32]);
        let envelope = encrypt(&key, &payload, &aad)?;
        let decrypted = decrypt(&key, &envelope, &aad)?;
        prop_assert_eq!(decrypted.expose_secret(), payload.as_slice());
    }

    #[test]
    fn mutated_ciphertext_never_decrypts(payload in proptest::collection::vec(any::<u8>(), 1..4096), aad in proptest::collection::vec(any::<u8>(), 0..512), index in any::<usize>()) {
        let key = VaultKey::from_bytes([43_u8; 32]);
        let mut envelope = encrypt(&key, &payload, &aad)?;
        let pos = index % envelope.ciphertext.len();
        envelope.ciphertext[pos] ^= 0x80;
        prop_assert!(decrypt(&key, &envelope, &aad).is_err());
    }

    #[test]
    fn downgraded_envelope_version_is_rejected(payload in proptest::collection::vec(any::<u8>(), 0..4096), aad in proptest::collection::vec(any::<u8>(), 0..512)) {
        let key = VaultKey::from_bytes([44_u8; 32]);
        let mut envelope: EncryptedEnvelope = encrypt(&key, &payload, &aad)?;
        envelope.version = 0;
        prop_assert!(decrypt(&key, &envelope, &aad).is_err());
    }
}
