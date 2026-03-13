//! Application bootstrap — first-run init and subsequent startup.

use crate::crypto::age::AgeCrypto;
use crate::domain::{AgeIdentity, SqliteConfig, Vault, VaultBackend};
use crate::error::{ConfigError, Error, Result};
use crate::storage::config::{default_data_dir, AppConfig};
use crate::storage::identity::{read_identity, write_identity};
use crate::storage::sqlite::SqliteStorage;
use crate::vault::registry::VaultRegistry;
use chrono::Utc;
use uuid::Uuid;

#[cfg(test)]
mod init_test;

/// Fully-bootstrapped application state.
///
/// Constructed via `App::init()` — either by running first-run setup or
/// by loading existing config and identity from disk.
pub struct App {
    pub config: AppConfig,
    pub identity: AgeIdentity,
    pub registry: VaultRegistry<SqliteStorage<AgeCrypto>, AgeCrypto>,
}

impl App {
    /// Bootstrap the application.
    ///
    /// On first run (no `config.toml`):
    /// 1. Generate a fresh age identity and write it to disk.
    /// 2. Create a default "Personal" vault backed by SQLite.
    /// 3. Persist the vault via `VaultRegistry`.
    /// 4. Write `config.toml` with the identity path and default vault id.
    ///
    /// On subsequent runs:
    /// 1. Load `config.toml`.
    /// 2. Read the identity from disk.
    /// 3. Open the default vault's SQLite storage.
    pub async fn init() -> Result<Self> {
        let config_path = AppConfig::default_path();

        match AppConfig::load(&config_path) {
            Ok(config) => Self::load_existing(config).await,
            Err(Error::Config(ConfigError::NotFound)) => Self::first_run(&config_path).await,
            Err(e) => Err(e),
        }
    }

    async fn first_run(config_path: &std::path::Path) -> Result<Self> {
        // 1. Generate identity and persist it
        let identity = AgeIdentity::generate();
        let identity_path = AppConfig::default_identity_path();
        write_identity(&identity, &identity_path)?;

        // 2. Derive recipient from the identity
        let recipient = identity.to_recipient();

        // 3. Build the default "Personal" vault
        let vault_id = Uuid::new_v4();
        let db_path = default_data_dir().join(format!("{}.db", vault_id));
        let vault = Vault {
            id: vault_id,
            name: "Personal".to_string(),
            is_default: true,
            created_at: Utc::now().timestamp(),
            backend: VaultBackend::Sqlite(SqliteConfig {
                path: db_path.to_string_lossy().to_string(),
            }),
            recipients: vec![recipient],
            lock_timeout: None,
            auto_sync: false,
        };

        // 4. Open SQLite storage and registry, persist the vault
        let crypto = AgeCrypto;
        let storage = SqliteStorage::new(&db_path, crypto).await?;
        let registry = VaultRegistry::new(storage, AgeCrypto);
        registry.add_vault(&vault).await?;

        // 5. Write config
        let config = AppConfig {
            identity_path,
            default_vault_id: Some(vault_id),
        };
        config.save(config_path)?;

        Ok(Self {
            config,
            identity,
            registry,
        })
    }

    async fn load_existing(config: AppConfig) -> Result<Self> {
        // Load identity from the path recorded in config
        let identity = read_identity(&config.identity_path)?;

        // Determine the default vault's db path
        let vault_id = config
            .default_vault_id
            .ok_or_else(|| ConfigError::ParseFailed("no default_vault_id in config".to_string()))?;

        let db_path = default_data_dir().join(format!("{}.db", vault_id));

        let crypto = AgeCrypto;
        let storage = SqliteStorage::new(&db_path, crypto).await?;
        let registry = VaultRegistry::new(storage, AgeCrypto);

        Ok(Self {
            config,
            identity,
            registry,
        })
    }
}
