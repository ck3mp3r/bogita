//! Tests for Age crypto implementation

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use super::*;
use crate::domain::AgeIdentity;

#[test]
fn test_age_crypto_creation() -> Result<()> {
    let crypto = AgeCrypto::new();
    // Should be able to create the crypto adapter
    assert!(std::mem::size_of_val(&crypto) == 0); // Zero-sized type
    Ok(())
}

#[test]
fn test_age_crypto_default() -> Result<()> {
    let crypto = AgeCrypto;
    assert!(std::mem::size_of_val(&crypto) == 0);
    Ok(())
}

#[test]
fn test_encrypt_decrypt_round_trip() -> Result<()> {
    let crypto = AgeCrypto::new();

    // Generate identity and recipient
    let identity = AgeIdentity::generate();
    let recipient = identity.to_recipient();

    // Test data
    let plaintext = b"Hello, Bogita!";

    // Encrypt
    let encrypted = crypto.encrypt(plaintext, &[recipient])?;

    // Verify encrypted data is different from plaintext
    assert_ne!(encrypted, plaintext);
    assert!(!encrypted.is_empty());

    // Decrypt with single identity
    let decrypted = crypto.decrypt(&encrypted, &identity)?;

    // Verify round trip
    assert_eq!(decrypted, plaintext);
    Ok(())
}

#[test]
fn test_encrypt_with_multiple_recipients() -> Result<()> {
    let crypto = AgeCrypto::new();

    // Generate three identities
    let identity1 = AgeIdentity::generate();
    let identity2 = AgeIdentity::generate();
    let identity3 = AgeIdentity::generate();

    let recipient1 = identity1.to_recipient();
    let recipient2 = identity2.to_recipient();
    let recipient3 = identity3.to_recipient();

    let plaintext = b"Multi-recipient message";

    // Encrypt with all three recipients
    let encrypted = crypto.encrypt(plaintext, &[recipient1, recipient2, recipient3])?;

    // Each identity should be able to decrypt independently
    let decrypted1 = crypto.decrypt(&encrypted, &identity1)?;
    assert_eq!(decrypted1, plaintext);

    let decrypted2 = crypto.decrypt(&encrypted, &identity2)?;
    assert_eq!(decrypted2, plaintext);

    let decrypted3 = crypto.decrypt(&encrypted, &identity3)?;
    assert_eq!(decrypted3, plaintext);
    Ok(())
}

#[test]
fn test_encrypt_empty_data() -> Result<()> {
    let crypto = AgeCrypto::new();
    let identity = AgeIdentity::generate();
    let recipient = identity.to_recipient();

    let plaintext = b"";

    let encrypted = crypto.encrypt(plaintext, &[recipient])?;

    let decrypted = crypto.decrypt(&encrypted, &identity)?;

    assert_eq!(decrypted, plaintext);
    Ok(())
}

#[test]
fn test_encrypt_large_data() -> Result<()> {
    let crypto = AgeCrypto::new();
    let identity = AgeIdentity::generate();
    let recipient = identity.to_recipient();

    // 1MB of data
    let plaintext: Vec<u8> = vec![42u8; 1024 * 1024];

    let encrypted = crypto.encrypt(&plaintext, &[recipient])?;

    let decrypted = crypto.decrypt(&encrypted, &identity)?;

    assert_eq!(decrypted, plaintext);
    Ok(())
}

#[test]
fn test_encrypt_with_no_recipients() -> Result<()> {
    let crypto = AgeCrypto::new();
    let plaintext = b"Test data";

    let result = crypto.encrypt(plaintext, &[]);

    assert!(result.is_err());
    match result {
        Err(e) => {
            // Should be CryptoError::InvalidRecipient
            assert!(e.to_string().contains("no recipients"));
        }
        Ok(_) => panic!("Should fail with no recipients"),
    }
    Ok(())
}

#[test]
fn test_decrypt_with_wrong_identity() -> Result<()> {
    let crypto = AgeCrypto::new();

    // Encrypt with one identity
    let identity1 = AgeIdentity::generate();
    let recipient1 = identity1.to_recipient();

    let plaintext = b"Secret data";
    let encrypted = crypto.encrypt(plaintext, &[recipient1])?;

    // Try to decrypt with different identity
    let identity2 = AgeIdentity::generate();
    let result = crypto.decrypt(&encrypted, &identity2);

    assert!(result.is_err());
    match result {
        Err(e) => {
            assert!(e.to_string().contains("decryption") || e.to_string().contains("Decryption"));
        }
        Ok(_) => panic!("Should fail with wrong identity"),
    }
    Ok(())
}

#[test]
fn test_decrypt_corrupted_data() -> Result<()> {
    let crypto = AgeCrypto::new();
    let identity = AgeIdentity::generate();

    // Completely invalid data
    let corrupted = b"This is not encrypted data";

    let result = crypto.decrypt(corrupted, &identity);

    assert!(result.is_err());
    Ok(())
}

#[test]
fn test_encrypt_field_value_json() -> Result<()> {
    use serde_json::json;

    let crypto = AgeCrypto::new();
    let identity = AgeIdentity::generate();
    let recipient = identity.to_recipient();

    // Simulate encrypting a FieldValue as JSON
    let field_value = json!({
        "type": "Hidden",
        "data": "my-secret-password"
    });

    let json_bytes = serde_json::to_vec(&field_value)?;

    let encrypted = crypto.encrypt(&json_bytes, &[recipient])?;

    let decrypted = crypto.decrypt(&encrypted, &identity)?;

    let decrypted_value: serde_json::Value = serde_json::from_slice(&decrypted)?;

    assert_eq!(decrypted_value, field_value);
    Ok(())
}

#[test]
fn test_multiple_encryptions_produce_different_ciphertexts() -> Result<()> {
    let crypto = AgeCrypto::new();
    let identity = AgeIdentity::generate();
    let recipient = identity.to_recipient();

    let plaintext = b"Same data encrypted twice";

    let encrypted1 = crypto.encrypt(plaintext, std::slice::from_ref(&recipient))?;

    let encrypted2 = crypto.encrypt(plaintext, std::slice::from_ref(&recipient))?;

    // age encryption includes random nonce, so ciphertexts should differ
    assert_ne!(encrypted1, encrypted2);

    // Both should decrypt to same plaintext
    let decrypted1 = crypto.decrypt(&encrypted1, &identity)?;
    let decrypted2 = crypto.decrypt(&encrypted2, &identity)?;

    assert_eq!(decrypted1, plaintext);
    assert_eq!(decrypted2, plaintext);
    Ok(())
}
