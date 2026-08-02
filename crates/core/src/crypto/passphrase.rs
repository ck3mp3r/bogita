//! Passphrase-based encryption and decryption using age's scrypt recipient.
//!
//! Wraps the age crate's passphrase API (`Encryptor::with_user_passphrase` /
//! `age::scrypt::Identity`) for encrypting/decrypting arbitrary byte payloads
//! with a human-memorable passphrase.

use crate::error::{CryptoError, Result};
use secrecy::SecretString;
use std::io::{Read, Write};
use std::iter;

/// Encrypt a plaintext string with a passphrase using age's scrypt passphrase
/// encryption.
///
/// Returns the encrypted bytes in age binary format (not armored).
pub fn encrypt_with_passphrase(plaintext: &str, passphrase: &SecretString) -> Result<Vec<u8>> {
    let encryptor = age::Encryptor::with_user_passphrase(passphrase.clone());

    let mut encrypted = vec![];
    let mut writer = encryptor
        .wrap_output(&mut encrypted)
        .map_err(|_| CryptoError::EncryptionFailed)?;

    writer
        .write_all(plaintext.as_bytes())
        .map_err(|_| CryptoError::EncryptionFailed)?;

    writer.finish().map_err(|_| CryptoError::EncryptionFailed)?;

    Ok(encrypted)
}

/// Decrypt a passphrase-encrypted blob and return the plaintext string.
///
/// Returns `CryptoError::DecryptionFailed` if the passphrase is wrong or the
/// data is corrupted.
pub fn decrypt_with_passphrase(encrypted: &[u8], passphrase: &SecretString) -> Result<String> {
    let decryptor = age::Decryptor::new(encrypted).map_err(|_| CryptoError::DecryptionFailed)?;

    let identity = age::scrypt::Identity::new(passphrase.clone());

    let mut decrypted = vec![];
    let mut reader = decryptor
        .decrypt(iter::once(&identity as &dyn age::Identity))
        .map_err(|_| CryptoError::DecryptionFailed)?;

    reader
        .read_to_end(&mut decrypted)
        .map_err(|_| CryptoError::DecryptionFailed)?;

    String::from_utf8(decrypted).map_err(|_| CryptoError::DecryptionFailed.into())
}

#[cfg(test)]
#[path = "passphrase_test.rs"]
mod passphrase_test;
