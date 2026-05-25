//! AEAD and KDF regression tests for ESPASS crypto-core.

use espass_crypto_core::{
    decrypt, derive_master_key, encrypt, envelope_version, KdfParams, Salt, StreamingDecryptor,
    StreamingEncryptor, VaultKey,
};

fn test_key() -> VaultKey {
    VaultKey::from_bytes([7_u8; 32])
}

#[test]
fn round_trip_encryption_with_associated_data() -> Result<(), espass_crypto_core::CryptoError> {
    let key = test_key();
    let aad = br#"tenant=t1;vault=v1;item=i1;type=login"#;
    let envelope = encrypt(&key, b"super secret", aad)?;

    assert_eq!(envelope.version, envelope_version());

    let plaintext = decrypt(&key, &envelope, aad)?;
    assert_eq!(plaintext.expose_secret(), b"super secret");
    Ok(())
}

#[test]
fn wrong_associated_data_fails() -> Result<(), espass_crypto_core::CryptoError> {
    let key = test_key();
    let envelope = encrypt(&key, b"super secret", b"vault=v1")?;
    let result = decrypt(&key, &envelope, b"vault=v2");

    assert_eq!(
        result,
        Err(espass_crypto_core::CryptoError::DecryptionFailed)
    );
    Ok(())
}

#[test]
fn tampered_ciphertext_fails() -> Result<(), espass_crypto_core::CryptoError> {
    let key = test_key();
    let mut envelope = encrypt(&key, b"super secret", b"vault=v1")?;
    envelope.ciphertext[0] ^= 0x01;

    let result = decrypt(&key, &envelope, b"vault=v1");
    assert_eq!(
        result,
        Err(espass_crypto_core::CryptoError::DecryptionFailed)
    );
    Ok(())
}

#[test]
fn encryption_uses_fresh_nonces() -> Result<(), espass_crypto_core::CryptoError> {
    let key = test_key();
    let first = encrypt(&key, b"same plaintext", b"same aad")?;
    let second = encrypt(&key, b"same plaintext", b"same aad")?;

    assert_ne!(first.nonce, second.nonce);
    assert_ne!(first.ciphertext, second.ciphertext);
    Ok(())
}

#[test]
fn unsupported_envelope_version_fails() -> Result<(), espass_crypto_core::CryptoError> {
    let key = test_key();
    let mut envelope = encrypt(&key, b"super secret", b"vault=v1")?;
    envelope.version = 0;

    let result = decrypt(&key, &envelope, b"vault=v1");
    assert_eq!(
        result,
        Err(espass_crypto_core::CryptoError::UnsupportedEnvelopeVersion)
    );
    Ok(())
}

#[test]
fn envelope_serializes_without_plaintext() -> Result<(), Box<dyn std::error::Error>> {
    let key = test_key();
    let envelope = encrypt(&key, b"super secret", b"vault=v1")?;
    let json = serde_json::to_string(&envelope)?;

    assert!(json.contains("\"version\""));
    assert!(!json.contains("super secret"));
    Ok(())
}

#[test]
fn kdf_derives_with_default_security_policy() -> Result<(), espass_crypto_core::CryptoError> {
    let salt = Salt::from_bytes(*b"0123456789abcdef");
    let key = derive_master_key(b"correct horse battery staple", &salt, KdfParams::default())?;

    assert_eq!(key.expose_secret().len(), 32);
    Ok(())
}

#[test]
fn kdf_rejects_downgraded_parameters() {
    let salt = Salt::from_bytes(*b"0123456789abcdef");
    let params = KdfParams {
        memory_cost_kib: 1024,
        iterations: 1,
        parallelism: 1,
    };

    let result = derive_master_key(b"password", &salt, params);
    assert_eq!(
        result.err(),
        Some(espass_crypto_core::CryptoError::InvalidKdfParameters)
    );
}

#[test]
fn streaming_chunks_bind_order_and_finality() -> Result<(), espass_crypto_core::CryptoError> {
    let key = test_key();
    let mut encryptor = StreamingEncryptor::new(&key, b"vault=v1;attachment=a1");
    let first = encryptor.encrypt_chunk(b"hello ", false)?;
    let second = encryptor.encrypt_chunk(b"world", true)?;

    let mut decryptor = StreamingDecryptor::new(&key, b"vault=v1;attachment=a1");
    let first_plaintext = decryptor.decrypt_chunk(&first, false)?;
    let second_plaintext = decryptor.decrypt_chunk(&second, true)?;

    assert_eq!(first_plaintext.expose_secret(), b"hello ");
    assert_eq!(second_plaintext.expose_secret(), b"world");

    let mut wrong_decryptor = StreamingDecryptor::new(&key, b"vault=v1;attachment=a1");
    let wrong = wrong_decryptor.decrypt_chunk(&second, true);
    assert_eq!(
        wrong.err(),
        Some(espass_crypto_core::CryptoError::DecryptionFailed)
    );

    Ok(())
}
