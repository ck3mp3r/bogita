//! Application bootstrap — first-run init and subsequent startup.

use crate::crypto::age::AgeCrypto;
use crate::domain::{AgeIdentity, SqliteConfig, Vault, VaultBackend};
use crate::error::{ConfigError, Error, Result};
use crate::storage::config::AppConfig;
use crate::storage::identity::{read_identity, write_identity};
use crate::storage::sqlite::SqliteStorage;
use crate::vault::registry::VaultRegistry;
use chrono::Utc;
use uuid::Uuid;

#[cfg(test)]
mod init_test;

/// Fully-bootstrapped application state.
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
    /// 4. Write `config.toml` with defaults (so the user can customise paths).
    ///
    /// On subsequent runs:
    /// 1. Load `config.toml`.
    /// 2. Resolve effective paths (overrides or XDG defaults).
    /// 3. Read the identity and open storage.
    pub async fn init() -> Result<Self> {
        let config_path = AppConfig::default_path();

        match AppConfig::load(&config_path) {
            Ok(config) => Self::load_existing(config).await,
            Err(Error::Config(ConfigError::NotFound)) => Self::first_run(&config_path).await,
            Err(e) => Err(e),
        }
    }

    async fn first_run(config_path: &std::path::Path) -> Result<Self> {
        let config = AppConfig::default();

        // 1. Generate identity and persist it
        let identity = AgeIdentity::generate();
        let identity_path = config.effective_identity_path();
        write_identity(&identity, &identity_path)?;

        // 2. Derive recipient from the identity
        let recipient = identity.to_recipient();

        // 3. Build the default "Personal" vault
        let vault_id = Uuid::new_v4();
        let db_path = config.effective_db_path();
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

        // 5. Write config so user can customise paths if desired
        config.save(config_path)?;

        Ok(Self {
            config,
            identity,
            registry,
        })
    }

    async fn load_existing(config: AppConfig) -> Result<Self> {
        let identity_path = config.effective_identity_path();
        let identity = read_identity(&identity_path)?;

        let db_path = config.effective_db_path();
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
