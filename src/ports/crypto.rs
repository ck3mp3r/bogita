//! Crypto port trait
//!
//! Defines the interface for encryption/decryption operations.

use crate::domain::{AgeIdentity, AgeRecipient};
use crate::error::Result;

/// Crypto port for encrypting and decrypting data
///
/// This trait defines the interface for encryption adapters.
/// All methods are synchronous as age encryption is CPU-bound, not I/O-bound.
pub trait Crypto: Send + Sync {
    /// Encrypt data with the given recipients
    ///
    /// Returns the encrypted data blob that can only be decrypted by
    /// any of the provided recipients' corresponding identities.
    fn encrypt(&self, data: &[u8], recipients: &[AgeRecipient]) -> Result<Vec<u8>>;

    /// Decrypt data using the given identity
    ///
    /// Returns the decrypted plaintext data if the identity can decrypt it.
    fn decrypt(&self, encrypted_data: &[u8], identity: &AgeIdentity) -> Result<Vec<u8>>;
}
