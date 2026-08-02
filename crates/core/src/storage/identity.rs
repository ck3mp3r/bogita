//! Identity persistence — write/read AgeIdentity to/from disk.
//!
//! The on-disk format is the age bech32 string (`AGE-SECRET-KEY-1...`)
//! written as plain text with 0o600 permissions. Passphrase-protected
//! variants are also available via `write_identity_encrypted` and
//! `read_identity_encrypted`.

use crate::domain::AgeIdentity;
use crate::error::{ConfigError, Result};
use secrecy::{ExposeSecret, SecretString};
use std::path::Path;
use std::str::FromStr;

/// Write an `AgeIdentity` to disk at `path`.
///
/// Creates parent directories if absent. Sets file permissions to 0o600
/// on Unix so the key is only readable by the owning user.
pub fn write_identity(identity: &AgeIdentity, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ConfigError::WriteFailed(e.to_string()))?;
    }

    let secret = identity.to_secret_string();
    std::fs::write(path, secret.expose_secret())
        .map_err(|e| ConfigError::WriteFailed(e.to_string()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| ConfigError::WriteFailed(e.to_string()))?;
    }

    Ok(())
}

/// Read an `AgeIdentity` from disk at `path`.
///
/// Returns `ConfigError::NotFound` if the file does not exist.
/// Returns `CryptoError::InvalidIdentity` if the content cannot be parsed.
pub fn read_identity(path: &Path) -> Result<AgeIdentity> {
    if !path.exists() {
        return Err(ConfigError::NotFound.into());
    }

    let contents =
        std::fs::read_to_string(path).map_err(|e| ConfigError::ParseFailed(e.to_string()))?;

    AgeIdentity::from_str(contents.trim())
        .map_err(|e| crate::error::CryptoError::InvalidIdentity(e).into())
}

/// Write an `AgeIdentity` to disk, encrypted with a passphrase.
///
/// The file format is age passphrase-encrypted bytes (not the raw bech32
/// string). Sets file permissions to 0o600 on Unix.
pub fn write_identity_encrypted(
    identity: &AgeIdentity,
    passphrase: &SecretString,
    path: &Path,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ConfigError::WriteFailed(e.to_string()))?;
    }

    let secret = identity.to_secret_string();
    let plaintext = secret.expose_secret();
    let encrypted = crate::crypto::passphrase::encrypt_with_passphrase(plaintext, passphrase)?;

    std::fs::write(path, &encrypted).map_err(|e| ConfigError::WriteFailed(e.to_string()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| ConfigError::WriteFailed(e.to_string()))?;
    }

    Ok(())
}

/// Read a passphrase-encrypted `AgeIdentity` from disk.
///
/// Returns `ConfigError::NotFound` if the file does not exist.
/// Returns `CryptoError::DecryptionFailed` if the passphrase is wrong.
/// Returns `CryptoError::InvalidIdentity` if the decrypted content cannot
/// be parsed as an age identity.
pub fn read_identity_encrypted(path: &Path, passphrase: &SecretString) -> Result<AgeIdentity> {
    if !path.exists() {
        return Err(ConfigError::NotFound.into());
    }

    let encrypted = std::fs::read(path).map_err(|e| ConfigError::ParseFailed(e.to_string()))?;

    let plaintext = crate::crypto::passphrase::decrypt_with_passphrase(&encrypted, passphrase)?;

    AgeIdentity::from_str(plaintext.trim())
        .map_err(|e| crate::error::CryptoError::InvalidIdentity(e).into())
}
