//! Configuration and path management for Bogita storage
//!
//! Handles XDG directory structure with separate dev/release directories.

use std::path::PathBuf;

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

/// Get the default database file path
///
/// Returns XDG_DATA_HOME/bogita/vault.db (or bogita-dev in debug builds)
pub fn default_db_path() -> PathBuf {
    default_data_dir().join("vault.db")
}

#[cfg(test)]
mod tests {
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
    fn test_db_path() {
        let db_path = default_db_path();
        assert!(db_path.ends_with("vault.db"));

        #[cfg(debug_assertions)]
        assert!(db_path.to_string_lossy().contains("bogita-dev"));
        #[cfg(not(debug_assertions))]
        {
            let path_str = db_path.to_string_lossy();
            assert!(path_str.contains("bogita"));
            assert!(!path_str.contains("bogita-dev"));
        }
    }
}
