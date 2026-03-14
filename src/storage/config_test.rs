use crate::error::{ConfigError, Error};
use crate::storage::config::{
    default_config_dir, default_data_dir, default_db_path, default_identity_path, AppConfig,
};
use std::path::PathBuf;
use tempfile::TempDir;

fn temp_config_path(dir: &TempDir) -> PathBuf {
    dir.path().join("config.toml")
}

#[test]
fn data_dir_ends_with_bogita_dev() {
    let data_dir = default_data_dir();
    #[cfg(debug_assertions)]
    assert!(data_dir.ends_with("bogita-dev"));
    #[cfg(not(debug_assertions))]
    assert!(data_dir.ends_with("bogita"));
}

#[test]
fn config_dir_ends_with_bogita_dev() {
    let config_dir = default_config_dir();
    #[cfg(debug_assertions)]
    assert!(config_dir.ends_with("bogita-dev"));
    #[cfg(not(debug_assertions))]
    assert!(config_dir.ends_with("bogita"));
}

#[test]
fn default_db_path_ends_with_vault_db() {
    let path = default_db_path();
    assert!(path.ends_with("vault.db"), "got: {}", path.display());
    #[cfg(debug_assertions)]
    assert!(path.to_string_lossy().contains("bogita-dev"));
}

#[test]
fn default_identity_path_ends_with_identity_age() {
    let path = default_identity_path();
    assert!(path.ends_with("identity.age"), "got: {}", path.display());
    #[cfg(debug_assertions)]
    assert!(path.to_string_lossy().contains("bogita-dev"));
}

#[test]
fn default_path_ends_with_config_toml() {
    let p = AppConfig::default_path();
    assert!(p.ends_with("config.toml"));
}

#[test]
fn effective_identity_path_falls_back_to_default() {
    let cfg = AppConfig::default();
    assert_eq!(cfg.effective_identity_path(), default_identity_path());
}

#[test]
fn effective_db_path_falls_back_to_default() {
    let cfg = AppConfig::default();
    assert_eq!(cfg.effective_db_path(), default_db_path());
}

#[test]
fn effective_identity_path_uses_override() {
    let cfg = AppConfig {
        identity_path: Some(PathBuf::from("/custom/identity.age")),
        db_path: None,
    };
    assert_eq!(
        cfg.effective_identity_path(),
        PathBuf::from("/custom/identity.age")
    );
}

#[test]
fn effective_db_path_uses_override() {
    let cfg = AppConfig {
        identity_path: None,
        db_path: Some(PathBuf::from("/custom/vault.db")),
    };
    assert_eq!(cfg.effective_db_path(), PathBuf::from("/custom/vault.db"));
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
fn load_parses_empty_toml_as_all_defaults() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);
    std::fs::write(&path, "").unwrap();

    let cfg = AppConfig::load(&path).unwrap();
    assert!(cfg.identity_path.is_none());
    assert!(cfg.db_path.is_none());
}

#[test]
fn load_parses_override_paths() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);
    let toml = r#"identity_path = "/custom/identity.age"
db_path = "/custom/vault.db"
"#;
    std::fs::write(&path, toml).unwrap();

    let cfg = AppConfig::load(&path).unwrap();
    assert_eq!(
        cfg.identity_path,
        Some(PathBuf::from("/custom/identity.age"))
    );
    assert_eq!(cfg.db_path, Some(PathBuf::from("/custom/vault.db")));
}

#[test]
fn save_default_writes_empty_toml() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);

    AppConfig::default().save(&path).unwrap();

    let contents = std::fs::read_to_string(&path).unwrap();
    // No override fields — TOML should be empty (no keys written)
    assert!(contents.trim().is_empty());
}

#[test]
fn save_round_trips_overrides() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);

    let original = AppConfig {
        identity_path: Some(PathBuf::from("/custom/identity.age")),
        db_path: Some(PathBuf::from("/custom/vault.db")),
    };
    original.save(&path).unwrap();

    let loaded = AppConfig::load(&path).unwrap();
    assert_eq!(loaded.identity_path, original.identity_path);
    assert_eq!(loaded.db_path, original.db_path);
}

#[test]
fn save_creates_parent_directories() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("a").join("b").join("config.toml");

    AppConfig::default().save(&nested).unwrap();
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
