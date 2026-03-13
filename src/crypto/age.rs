//! Age encryption implementation
//!
//! Implements the Crypto port trait using age encryption for field-level encryption.
//!
//! Note: The age API requires `&dyn Recipient` and `&dyn Identity` trait references
//! in iterators. We use stack references (not Box<dyn>) to minimize heap allocations.

use crate::domain::{AgeIdentity, AgeRecipient};
use crate::error::{CryptoError, Result};
use crate::ports::Crypto;
use std::io::{Read, Write};

/// Age encryption adapter (zero-sized type for zero-cost abstraction)
pub struct AgeCrypto;

impl AgeCrypto {
    /// Create a new Age crypto adapter
    pub fn new() -> Self {
        Self
    }
}

impl Default for AgeCrypto {
    fn default() -> Self {
        Self::new()
    }
}

impl Crypto for AgeCrypto {
    fn encrypt(&self, data: &[u8], recipients: &[AgeRecipient]) -> Result<Vec<u8>> {
        if recipients.is_empty() {
            return Err(CryptoError::InvalidRecipient("no recipients provided".to_string()).into());
        }

        // Create encryptor with recipients
        // age API requires &dyn Recipient - we use stack references for zero-cost
        let encryptor = age::Encryptor::with_recipients(
            recipients.iter().map(|r| r.inner() as &dyn age::Recipient),
        )
        .map_err(|_| CryptoError::EncryptionFailed)?;

        // Encrypt data
        let mut encrypted = vec![];
        let mut writer = encryptor
            .wrap_output(&mut encrypted)
            .map_err(|_| CryptoError::EncryptionFailed)?;

        writer
            .write_all(data)
            .map_err(|_| CryptoError::EncryptionFailed)?;

        writer.finish().map_err(|_| CryptoError::EncryptionFailed)?;

        Ok(encrypted)
    }

    fn decrypt(&self, encrypted: &[u8], identity: &AgeIdentity) -> Result<Vec<u8>> {
        // Decrypt with the provided identity
        // age API requires &dyn Identity - we use stack reference for zero-cost
        let decryptor =
            age::Decryptor::new(encrypted).map_err(|_| CryptoError::DecryptionFailed)?;

        let mut decrypted = vec![];
        let mut reader = decryptor
            .decrypt(std::iter::once(identity.inner() as &dyn age::Identity))
            .map_err(|_| CryptoError::DecryptionFailed)?;

        reader
            .read_to_end(&mut decrypted)
            .map_err(|_| CryptoError::DecryptionFailed)?;

        Ok(decrypted)
    }
}

#[cfg(test)]
#[path = "age_test.rs"]
mod age_test;
