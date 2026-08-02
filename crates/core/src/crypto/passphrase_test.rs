use crate::crypto::passphrase::{decrypt_with_passphrase, encrypt_with_passphrase};
use secrecy::SecretString;
use std::io::Write;
use std::iter;

/// Helper: encrypt with a low scrypt work factor for fast tests.
fn encrypt_fast(plaintext: &str, passphrase: &SecretString) -> Vec<u8> {
    let mut recipient = age::scrypt::Recipient::new(passphrase.clone());
    recipient.set_work_factor(2); // Very fast for testing

    let encryptor = age::Encryptor::with_recipients(iter::once(&recipient as &dyn age::Recipient))
        .expect("single recipient should work");

    let mut encrypted = vec![];
    let mut writer = encryptor.wrap_output(&mut encrypted).unwrap();
    writer.write_all(plaintext.as_bytes()).unwrap();
    writer.finish().unwrap();
    encrypted
}

#[test]
fn encrypt_decrypt_roundtrip() {
    let passphrase = SecretString::from("correct horse battery staple");
    let plaintext = "AGE-SECRET-KEY-1TEST...";

    let encrypted = encrypt_with_passphrase(plaintext, &passphrase).unwrap();
    let decrypted = decrypt_with_passphrase(&encrypted, &passphrase).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[test]
fn wrong_passphrase_fails() {
    let passphrase = SecretString::from("correct horse battery staple");
    let wrong = SecretString::from("wrong passphrase");

    let encrypted = encrypt_fast("secret", &passphrase);
    let result = decrypt_with_passphrase(&encrypted, &wrong);
    assert!(result.is_err(), "expected error for wrong passphrase");
}

#[test]
fn empty_plaintext_roundtrips() {
    let passphrase = SecretString::from("pass");

    let encrypted = encrypt_with_passphrase("", &passphrase).unwrap();
    let decrypted = decrypt_with_passphrase(&encrypted, &passphrase).unwrap();

    assert_eq!(decrypted, "");
}

#[test]
fn corrupted_data_fails() {
    let passphrase = SecretString::from("pass");
    let encrypted = encrypt_fast("some data", &passphrase);

    // Corrupt the last byte
    let mut corrupted = encrypted.clone();
    if let Some(byte) = corrupted.last_mut() {
        *byte ^= 0xff;
    }

    let result = decrypt_with_passphrase(&corrupted, &passphrase);
    assert!(result.is_err(), "expected error for corrupted data");
}
