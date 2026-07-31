//! Configuration and path management for Bogita storage
//!
//! Follows the XDG Base Directory Specification via the `xdg` crate.
//! Debug builds use a `-dev` suffix to keep dev/release data separate.

use crate::error::{ConfigError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// App name used as the XDG prefix, varies by build profile.
#[cfg(debug_assertions)]
const APP_NAME: &str = "bogita-dev";

#[cfg(not(debug_assertions))]
const APP_NAME: &str = "bogita";

fn xdg_dirs() -> xdg::BaseDirectories {
    xdg::BaseDirectories::with_prefix(APP_NAME)
}

/// `$XDG_DATA_HOME/bogita[-dev]/`  (default: `~/.local/share/bogita[-dev]/`)
pub fn default_data_dir() -> PathBuf {
    xdg_dirs()
        .get_data_home()
        .expect("failed to resolve XDG data home")
}

/// `$XDG_CONFIG_HOME/bogita[-dev]/`  (default: `~/.config/bogita[-dev]/`)
pub fn default_config_dir() -> PathBuf {
    xdg_dirs()
        .get_config_home()
        .expect("failed to resolve XDG config home")
}

/// `$XDG_DATA_HOME/bogita[-dev]/vault.db`
pub fn default_db_path() -> PathBuf {
    default_data_dir().join("vault.db")
}

/// `$XDG_DATA_HOME/bogita[-dev]/identity.age`
pub fn default_identity_path() -> PathBuf {
    default_data_dir().join("identity.age")
}

/// Root application configuration.
///
/// Written on first run with sane defaults so the user can inspect and
/// customise it. Any field left absent (or commented out) in the TOML
/// falls back to its XDG-derived default.
///
/// Persisted as TOML at `default_config_dir()/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// Override the age identity file path.
    /// Defaults to `XDG_DATA_HOME/bogita[-dev]/identity.age`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_path: Option<PathBuf>,

    /// Override the SQLite vault database path.
    /// Defaults to `XDG_DATA_HOME/bogita[-dev]/vault.db`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_path: Option<PathBuf>,
}

impl AppConfig {
    /// Returns the canonical path for the config file.
    ///
    /// `XDG_CONFIG_HOME/bogita[-dev]/config.toml`
    pub fn default_path() -> PathBuf {
        default_config_dir().join("config.toml")
    }

    /// Resolve the effective identity path (override or XDG default).
    pub fn effective_identity_path(&self) -> PathBuf {
        self.identity_path
            .clone()
            .unwrap_or_else(default_identity_path)
    }

    /// Resolve the effective database path (override or XDG default).
    pub fn effective_db_path(&self) -> PathBuf {
        self.db_path.clone().unwrap_or_else(default_db_path)
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
