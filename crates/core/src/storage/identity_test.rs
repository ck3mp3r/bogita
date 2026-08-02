use crate::domain::AgeIdentity;
use crate::error::{ConfigError, Error};
use crate::storage::identity::{
    read_identity, read_identity_encrypted, write_identity, write_identity_encrypted,
};
use secrecy::SecretString;
use tempfile::TempDir;

#[test]
fn write_then_read_round_trips() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.age");

    let identity = AgeIdentity::generate();
    let recipient_before = identity.to_recipient().to_string();

    write_identity(&identity, &path).unwrap();

    let loaded = read_identity(&path).unwrap();
    let recipient_after = loaded.to_recipient().to_string();

    assert_eq!(recipient_before, recipient_after);
}

#[test]
fn read_returns_not_found_for_missing_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nonexistent.age");

    let err = read_identity(&path).unwrap_err();
    assert!(
        matches!(err, Error::Config(ConfigError::NotFound)),
        "expected ConfigError::NotFound, got: {err}"
    );
}

#[test]
fn write_creates_parent_directories() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested").join("dir").join("identity.age");

    let identity = AgeIdentity::generate();
    write_identity(&identity, &path).unwrap();
    assert!(path.exists());
}

#[cfg(unix)]
#[test]
fn written_file_has_0600_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.age");

    let identity = AgeIdentity::generate();
    write_identity(&identity, &path).unwrap();

    let perms = std::fs::metadata(&path).unwrap().permissions();
    assert_eq!(
        perms.mode() & 0o777,
        0o600,
        "expected 0o600, got 0o{:o}",
        perms.mode() & 0o777
    );
}

// ---------------------------------------------------------------------------
// Encrypted identity tests
// ---------------------------------------------------------------------------

#[test]
fn encrypted_identity_roundtrip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.age");
    let identity = AgeIdentity::generate();
    let passphrase = SecretString::from("my passphrase");

    write_identity_encrypted(&identity, &passphrase, &path).unwrap();
    let loaded = read_identity_encrypted(&path, &passphrase).unwrap();

    assert_eq!(identity.to_recipient(), loaded.to_recipient());
}

#[test]
fn encrypted_identity_wrong_passphrase_fails() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.age");
    let identity = AgeIdentity::generate();
    let passphrase = SecretString::from("correct");

    write_identity_encrypted(&identity, &passphrase, &path).unwrap();

    let wrong = SecretString::from("wrong");
    let result = read_identity_encrypted(&path, &wrong);
    assert!(result.is_err(), "expected error for wrong passphrase");
}

#[test]
fn encrypted_identity_not_found() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nonexistent.age");
    let passphrase = SecretString::from("pass");

    let err = read_identity_encrypted(&path, &passphrase).unwrap_err();
    assert!(
        matches!(err, Error::Config(ConfigError::NotFound)),
        "expected ConfigError::NotFound, got: {err}"
    );
}

#[cfg(unix)]
#[test]
fn encrypted_identity_file_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.age");
    let identity = AgeIdentity::generate();
    let passphrase = SecretString::from("pass");

    write_identity_encrypted(&identity, &passphrase, &path).unwrap();

    let perms = std::fs::metadata(&path).unwrap().permissions();
    assert_eq!(
        perms.mode() & 0o777,
        0o600,
        "expected 0o600, got 0o{:o}",
        perms.mode() & 0o777
    );
}
