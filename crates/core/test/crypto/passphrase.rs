use crate::crypto::passphrase::{decrypt_with_passphrase, encrypt_with_passphrase};
use secrecy::SecretString;
use std::io::Write;
use std::iter;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

/// Helper: encrypt with a low scrypt work factor for fast tests.
fn encrypt_fast(plaintext: &str, passphrase: &SecretString) -> Result<Vec<u8>> {
    let mut recipient = age::scrypt::Recipient::new(passphrase.clone());
    recipient.set_work_factor(2); // Very fast for testing

    let encryptor = age::Encryptor::with_recipients(iter::once(&recipient as &dyn age::Recipient))?;

    let mut encrypted = vec![];
    let mut writer = encryptor.wrap_output(&mut encrypted)?;
    writer.write_all(plaintext.as_bytes())?;
    writer.finish()?;
    Ok(encrypted)
}

#[test]
fn encrypt_decrypt_roundtrip() -> Result<()> {
    // -- Setup & Fixtures
    let passphrase = SecretString::from("correct horse battery staple");
    let plaintext = "AGE-SECRET-KEY-1TEST...";

    // -- Exec
    let encrypted = encrypt_with_passphrase(plaintext, &passphrase)?;
    let decrypted = decrypt_with_passphrase(&encrypted, &passphrase)?;

    // -- Check
    assert_eq!(decrypted, plaintext);
    Ok(())
}

#[test]
fn wrong_passphrase_fails() -> Result<()> {
    // -- Setup & Fixtures
    let passphrase = SecretString::from("correct horse battery staple");
    let wrong = SecretString::from("wrong passphrase");
    let encrypted = encrypt_fast("secret", &passphrase)?;

    // -- Exec
    let result = decrypt_with_passphrase(&encrypted, &wrong);

    // -- Check
    assert!(result.is_err(), "expected error for wrong passphrase");
    Ok(())
}

#[test]
fn empty_plaintext_roundtrips() -> Result<()> {
    // -- Setup & Fixtures
    let passphrase = SecretString::from("pass");

    // -- Exec
    let encrypted = encrypt_with_passphrase("", &passphrase)?;
    let decrypted = decrypt_with_passphrase(&encrypted, &passphrase)?;

    // -- Check
    assert_eq!(decrypted, "");
    Ok(())
}

#[test]
fn corrupted_data_fails() -> Result<()> {
    // -- Setup & Fixtures
    let passphrase = SecretString::from("pass");
    let encrypted = encrypt_fast("some data", &passphrase)?;
    let mut corrupted = encrypted.clone();
    if let Some(byte) = corrupted.last_mut() {
        *byte ^= 0xff;
    }

    // -- Exec
    let result = decrypt_with_passphrase(&corrupted, &passphrase);

    // -- Check
    assert!(result.is_err(), "expected error for corrupted data");
    Ok(())
}
