type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::error::{ConfigError, Error};
use crate::storage::config::{
    default_config_dir, default_data_dir, default_db_path, default_identity_path, AppConfig,
};
use serial_test::serial;
use std::path::PathBuf;
use tempfile::TempDir;

#[cfg(debug_assertions)]
const APP_DIR_NAME: &str = "bogita-dev";
#[cfg(not(debug_assertions))]
const APP_DIR_NAME: &str = "bogita";

fn temp_config_path(dir: &TempDir) -> PathBuf {
    dir.path().join("config.toml")
}

#[test]
fn data_dir_ends_with_bogita_dev() -> Result<()> {
    let data_dir = default_data_dir();
    #[cfg(debug_assertions)]
    assert!(data_dir.ends_with("bogita-dev"));
    #[cfg(not(debug_assertions))]
    assert!(data_dir.ends_with("bogita"));
    Ok(())
}

#[test]
fn config_dir_ends_with_bogita_dev() -> Result<()> {
    let config_dir = default_config_dir();
    #[cfg(debug_assertions)]
    assert!(config_dir.ends_with("bogita-dev"));
    #[cfg(not(debug_assertions))]
    assert!(config_dir.ends_with("bogita"));
    Ok(())
}

#[test]
fn default_db_path_ends_with_vault_db() -> Result<()> {
    let path = default_db_path();
    assert!(path.ends_with("vault.db"), "got: {}", path.display());
    #[cfg(debug_assertions)]
    assert!(path.to_string_lossy().contains("bogita-dev"));
    Ok(())
}

#[test]
fn default_identity_path_ends_with_identity_age() -> Result<()> {
    let path = default_identity_path();
    assert!(path.ends_with("identity.age"), "got: {}", path.display());
    #[cfg(debug_assertions)]
    assert!(path.to_string_lossy().contains("bogita-dev"));
    Ok(())
}

#[test]
fn default_path_ends_with_config_toml() -> Result<()> {
    let p = AppConfig::default_path();
    assert!(p.ends_with("config.toml"));
    Ok(())
}

#[test]
fn effective_identity_path_falls_back_to_default() -> Result<()> {
    let cfg = AppConfig::default();
    assert_eq!(cfg.effective_identity_path(), default_identity_path());
    Ok(())
}

#[test]
fn effective_db_path_falls_back_to_default() -> Result<()> {
    let cfg = AppConfig::default();
    assert_eq!(cfg.effective_db_path(), default_db_path());
    Ok(())
}

#[test]
fn effective_identity_path_uses_override() -> Result<()> {
    let cfg = AppConfig {
        identity_path: Some(PathBuf::from("/custom/identity.age")),
        db_path: None,
    };
    assert_eq!(
        cfg.effective_identity_path(),
        PathBuf::from("/custom/identity.age")
    );
    Ok(())
}

#[test]
fn effective_db_path_uses_override() -> Result<()> {
    let cfg = AppConfig {
        identity_path: None,
        db_path: Some(PathBuf::from("/custom/vault.db")),
    };
    assert_eq!(cfg.effective_db_path(), PathBuf::from("/custom/vault.db"));
    Ok(())
}

#[test]
fn load_returns_not_found_when_absent() -> Result<()> {
    let dir = TempDir::new()?;
    let path = temp_config_path(&dir);
    let err = AppConfig::load(&path).unwrap_err();
    assert!(
        matches!(err, Error::Config(ConfigError::NotFound)),
        "expected ConfigError::NotFound, got: {err}"
    );
    Ok(())
}

#[test]
fn load_parses_empty_toml_as_all_defaults() -> Result<()> {
    let dir = TempDir::new()?;
    let path = temp_config_path(&dir);
    std::fs::write(&path, "")?;

    let cfg = AppConfig::load(&path)?;
    assert!(cfg.identity_path.is_none());
    assert!(cfg.db_path.is_none());
    Ok(())
}

#[test]
fn load_parses_override_paths() -> Result<()> {
    let dir = TempDir::new()?;
    let path = temp_config_path(&dir);
    let toml = r#"identity_path = "/custom/identity.age"
db_path = "/custom/vault.db"
"#;
    std::fs::write(&path, toml)?;

    let cfg = AppConfig::load(&path)?;
    assert_eq!(
        cfg.identity_path,
        Some(PathBuf::from("/custom/identity.age"))
    );
    assert_eq!(cfg.db_path, Some(PathBuf::from("/custom/vault.db")));
    Ok(())
}

#[test]
fn save_default_writes_empty_toml() -> Result<()> {
    let dir = TempDir::new()?;
    let path = temp_config_path(&dir);

    AppConfig::default().save(&path)?;

    let contents = std::fs::read_to_string(&path)?;
    // No override fields — TOML should be empty (no keys written)
    assert!(contents.trim().is_empty());
    Ok(())
}

#[test]
fn save_round_trips_overrides() -> Result<()> {
    let dir = TempDir::new()?;
    let path = temp_config_path(&dir);

    let original = AppConfig {
        identity_path: Some(PathBuf::from("/custom/identity.age")),
        db_path: Some(PathBuf::from("/custom/vault.db")),
    };
    original.save(&path)?;

    let loaded = AppConfig::load(&path)?;
    assert_eq!(loaded.identity_path, original.identity_path);
    assert_eq!(loaded.db_path, original.db_path);
    Ok(())
}

#[test]
fn save_creates_parent_directories() -> Result<()> {
    let dir = TempDir::new()?;
    let nested = dir.path().join("a").join("b").join("config.toml");

    AppConfig::default().save(&nested)?;
    assert!(nested.exists());
    Ok(())
}

#[test]
fn load_returns_parse_failed_for_bad_toml() -> Result<()> {
    let dir = TempDir::new()?;
    let path = temp_config_path(&dir);
    std::fs::write(&path, "this is not valid toml !!!##")?;

    let err = AppConfig::load(&path).unwrap_err();
    assert!(
        matches!(err, Error::Config(ConfigError::ParseFailed(_))),
        "expected ConfigError::ParseFailed, got: {err}"
    );
    Ok(())
}

#[test]
#[serial]
fn data_dir_falls_back_to_home_when_xdg_data_home_is_unset() -> Result<()> {
    let home = TempDir::new()?;
    let previous_xdg = std::env::var_os("XDG_DATA_HOME");
    let previous_home = std::env::var_os("HOME");
    std::env::remove_var("XDG_DATA_HOME");
    std::env::set_var("HOME", home.path());

    let data_dir = default_data_dir();

    match previous_xdg {
        Some(value) => std::env::set_var("XDG_DATA_HOME", value),
        None => std::env::remove_var("XDG_DATA_HOME"),
    }
    match previous_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }

    assert_eq!(
        data_dir,
        home.path().join(".local").join("share").join(APP_DIR_NAME),
        "unset XDG_DATA_HOME must fall back to $HOME/.local/share"
    );
    Ok(())
}

#[test]
#[serial]
fn config_dir_falls_back_to_home_when_xdg_config_home_is_unset() -> Result<()> {
    let home = TempDir::new()?;
    let previous_xdg = std::env::var_os("XDG_CONFIG_HOME");
    let previous_home = std::env::var_os("HOME");
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::set_var("HOME", home.path());

    let config_dir = default_config_dir();

    match previous_xdg {
        Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }
    match previous_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }

    assert_eq!(
        config_dir,
        home.path().join(".config").join(APP_DIR_NAME),
        "unset XDG_CONFIG_HOME must fall back to $HOME/.config"
    );
    Ok(())
}
