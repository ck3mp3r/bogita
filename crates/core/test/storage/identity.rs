type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::domain::AgeIdentity;
use crate::error::{ConfigError, Error};
use crate::storage::identity::{read_identity_encrypted, write_identity_encrypted};
use secrecy::SecretString;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Encrypted identity tests
// ---------------------------------------------------------------------------

#[test]
fn encrypted_identity_roundtrip() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("identity.age");
    let identity = AgeIdentity::generate();
    let passphrase = SecretString::from("my passphrase");

    write_identity_encrypted(&identity, &passphrase, &path)?;
    let loaded = read_identity_encrypted(&path, &passphrase)?;

    assert_eq!(identity.to_recipient(), loaded.to_recipient());
    Ok(())
}

#[test]
fn encrypted_identity_wrong_passphrase_fails() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("identity.age");
    let identity = AgeIdentity::generate();
    let passphrase = SecretString::from("correct");

    write_identity_encrypted(&identity, &passphrase, &path)?;

    let wrong = SecretString::from("wrong");
    let result = read_identity_encrypted(&path, &wrong);
    assert!(result.is_err(), "expected error for wrong passphrase");
    Ok(())
}

#[test]
fn encrypted_identity_not_found() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("nonexistent.age");
    let passphrase = SecretString::from("pass");

    let err = read_identity_encrypted(&path, &passphrase).unwrap_err();
    assert!(
        matches!(err, Error::Config(ConfigError::NotFound)),
        "expected ConfigError::NotFound, got: {err}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn encrypted_identity_file_permissions() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new()?;
    let path = dir.path().join("identity.age");
    let identity = AgeIdentity::generate();
    let passphrase = SecretString::from("pass");

    write_identity_encrypted(&identity, &passphrase, &path)?;

    let perms = std::fs::metadata(&path)?.permissions();
    assert_eq!(
        perms.mode() & 0o777,
        0o600,
        "expected 0o600, got 0o{:o}",
        perms.mode() & 0o777
    );
    Ok(())
}
