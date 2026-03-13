//! Configuration and path management for Bogita storage
//!
//! Handles XDG directory structure with separate dev/release directories.

use crate::error::{ConfigError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

// Directory names based on build mode
#[cfg(debug_assertions)]
const DATA_DIR_NAME: &str = "bogita-dev";

#[cfg(not(debug_assertions))]
const DATA_DIR_NAME: &str = "bogita";

#[cfg(debug_assertions)]
const CONFIG_DIR_NAME: &str = "bogita-dev";

#[cfg(not(debug_assertions))]
const CONFIG_DIR_NAME: &str = "bogita";

/// Get the default database directory path
///
/// Uses XDG_DATA_HOME/bogita (or bogita-dev in debug builds)
pub fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .expect("Failed to determine XDG_DATA_HOME")
        .join(DATA_DIR_NAME)
}

/// Get the default configuration directory path
///
/// Uses XDG_CONFIG_HOME/bogita (or bogita-dev in debug builds)
pub fn default_config_dir() -> PathBuf {
    dirs::config_dir()
        .expect("Failed to determine XDG_CONFIG_HOME")
        .join(CONFIG_DIR_NAME)
}

/// Get the SQLite database file path for a specific vault.
///
/// Returns `XDG_DATA_HOME/bogita[-dev]/<vault_id>.db`
pub fn vault_db_path(vault_id: Uuid) -> PathBuf {
    default_data_dir().join(format!("{}.db", vault_id))
}

/// Root application configuration — the anchor for all paths.
///
/// Persisted as TOML at `default_config_dir()/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Path to the age identity file (private key)
    pub identity_path: PathBuf,

    /// UUID of the default vault (None before first-run init)
    pub default_vault_id: Option<Uuid>,
}

impl AppConfig {
    /// Returns the canonical path for the config file.
    ///
    /// `XDG_CONFIG_HOME/bogita[-dev]/config.toml`
    pub fn default_path() -> PathBuf {
        default_config_dir().join("config.toml")
    }

    /// Returns the canonical path for the age identity file.
    ///
    /// `XDG_DATA_HOME/bogita[-dev]/identity.age`
    pub fn default_identity_path() -> PathBuf {
        default_data_dir().join("identity.age")
    }

    /// Load config from `path`.
    ///
    /// Returns `ConfigError::NotFound` if the file does not exist,
    /// `ConfigError::ParseFailed` if the TOML is malformed.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(ConfigError::NotFound.into());
        }
        let contents =
            std::fs::read_to_string(path).map_err(|e| ConfigError::ParseFailed(e.to_string()))?;
        toml::from_str(&contents).map_err(|e| ConfigError::ParseFailed(e.to_string()).into())
    }

    /// Save config to `path` atomically (write tmp → rename).
    ///
    /// Creates parent directories if they don't exist.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ConfigError::WriteFailed(e.to_string()))?;
        }
        let contents =
            toml::to_string_pretty(self).map_err(|e| ConfigError::WriteFailed(e.to_string()))?;

        // Atomic write: write to a tmp file alongside target, then rename
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, &contents).map_err(|e| ConfigError::WriteFailed(e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| ConfigError::WriteFailed(e.to_string()).into())
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn test_data_dir_name() {
        let data_dir = default_data_dir();
        #[cfg(debug_assertions)]
        assert!(data_dir.ends_with("bogita-dev"));
        #[cfg(not(debug_assertions))]
        assert!(data_dir.ends_with("bogita"));
    }

    #[test]
    fn test_config_dir_name() {
        let config_dir = default_config_dir();
        #[cfg(debug_assertions)]
        assert!(config_dir.ends_with("bogita-dev"));
        #[cfg(not(debug_assertions))]
        assert!(config_dir.ends_with("bogita"));
    }

    #[test]
    fn test_vault_db_path() {
        let id = Uuid::new_v4();
        let path = vault_db_path(id);
        assert!(path.to_string_lossy().ends_with(&format!("{}.db", id)));
        #[cfg(debug_assertions)]
        assert!(path.to_string_lossy().contains("bogita-dev"));
        #[cfg(not(debug_assertions))]
        {
            let s = path.to_string_lossy();
            assert!(s.contains("bogita"));
            assert!(!s.contains("bogita-dev"));
        }
    }

    #[test]
    fn default_path_ends_with_config_toml() {
        let p = AppConfig::default_path();
        assert!(p.ends_with("config.toml"));
    }

    #[test]
    fn default_identity_path_ends_with_identity_age() {
        let p = AppConfig::default_identity_path();
        assert!(p.ends_with("identity.age"));
    }
}
