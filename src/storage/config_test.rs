use crate::error::{ConfigError, Error};
use crate::storage::config::AppConfig;
use std::path::PathBuf;
use tempfile::TempDir;
use uuid::Uuid;

fn temp_config_path(dir: &TempDir) -> PathBuf {
    dir.path().join("config.toml")
}

#[test]
fn load_returns_not_found_when_absent() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);
    let err = AppConfig::load(&path).unwrap_err();
    assert!(
        matches!(err, Error::Config(ConfigError::NotFound)),
        "expected ConfigError::NotFound, got: {err}"
    );
}

#[test]
fn load_parses_valid_toml() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);
    let id = Uuid::new_v4();
    let toml = format!(
        r#"identity_path = "/tmp/identity.age"
default_vault_id = "{}"
"#,
        id
    );
    std::fs::write(&path, toml).unwrap();

    let cfg = AppConfig::load(&path).unwrap();
    assert_eq!(cfg.identity_path, std::path::Path::new("/tmp/identity.age"));
    assert_eq!(cfg.default_vault_id, Some(id));
}

#[test]
fn save_round_trips_through_load() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);
    let id = Uuid::new_v4();

    let original = AppConfig {
        identity_path: PathBuf::from("/some/path/identity.age"),
        default_vault_id: Some(id),
    };
    original.save(&path).unwrap();

    let loaded = AppConfig::load(&path).unwrap();
    assert_eq!(loaded.identity_path, original.identity_path);
    assert_eq!(loaded.default_vault_id, original.default_vault_id);
}

#[test]
fn save_creates_parent_directories() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("a").join("b").join("config.toml");

    let cfg = AppConfig {
        identity_path: PathBuf::from("/tmp/identity.age"),
        default_vault_id: None,
    };
    cfg.save(&nested).unwrap();
    assert!(nested.exists());
}

#[test]
fn load_returns_parse_failed_for_bad_toml() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);
    std::fs::write(&path, "this is not valid toml !!!##").unwrap();

    let err = AppConfig::load(&path).unwrap_err();
    assert!(
        matches!(err, Error::Config(ConfigError::ParseFailed(_))),
        "expected ConfigError::ParseFailed, got: {err}"
    );
}
