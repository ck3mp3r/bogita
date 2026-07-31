use crate::domain::AgeIdentity;
use crate::error::{ConfigError, Error};
use crate::storage::identity::{read_identity, write_identity};
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
