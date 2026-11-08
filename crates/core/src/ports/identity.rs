//! Identity store port — reads and writes the encrypted age identity file.

use crate::domain::AgeIdentity;
use crate::error::Result;
use crate::storage::identity::{read_identity_encrypted, write_identity_encrypted};
use secrecy::SecretString;
use std::path::Path;

/// Persists the passphrase-encrypted age identity.
///
/// The app only knows this trait, so tests can inject an identity store
/// that keeps the identity in memory instead of on disk.
pub trait IdentityStore {
    /// Read and decrypt the identity stored at `path`.
    fn read_encrypted(&self, path: &Path, passphrase: &SecretString) -> Result<AgeIdentity>;

    /// Encrypt `identity` with `passphrase` and write it to `path`.
    fn write_encrypted(
        &self,
        identity: &AgeIdentity,
        passphrase: &SecretString,
        path: &Path,
    ) -> Result<()>;
}

/// Passphrase-encrypted identity file used by the production composition root.
#[derive(Clone, Copy, Default)]
pub struct EncryptedIdentityFile;

impl IdentityStore for EncryptedIdentityFile {
    fn read_encrypted(&self, path: &Path, passphrase: &SecretString) -> Result<AgeIdentity> {
        read_identity_encrypted(path, passphrase)
    }

    fn write_encrypted(
        &self,
        identity: &AgeIdentity,
        passphrase: &SecretString,
        path: &Path,
    ) -> Result<()> {
        write_identity_encrypted(identity, passphrase, path)
    }
}
