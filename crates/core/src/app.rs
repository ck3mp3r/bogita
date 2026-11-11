//! Application bootstrap — first-run init and subsequent startup.
//!
//! The lock lives in memory. No credential store is consulted at boot, so an
//! existing vault always boots locked and the caller derives the identity from
//! the encrypted identity file with a passphrase.

use crate::domain::{AgeIdentity, Vault};
use crate::error::{Error, Result};
use crate::ports::{
    EncryptedIdentityFile, IdentityStore, SqliteStorageFactory, VaultStorageFactory,
};
use crate::storage::config::AppConfig;
use crate::vault::registry::VaultRegistry;
use chrono::Utc;
use secrecy::SecretString;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Fully wired application type used by the binary and the TUI.
pub type DefaultApp = App<SqliteStorageFactory, EncryptedIdentityFile>;

/// Result of [`App::init()`] — the caller must handle each variant.
pub enum InitResult<F: VaultStorageFactory, I: IdentityStore> {
    /// First run: no identity file exists. Caller must prompt for a passphrase
    /// and call [`App::complete_first_run`].
    NeedsPassphrase(InitParts<F, I>),
    /// Encrypted identity found and no identity held: the vault is locked.
    /// Caller must prompt for a passphrase and call [`App::complete_unlock`].
    Locked(InitParts<F, I>),
    /// Initialization failed and cannot be recovered (for example, no usable database).
    /// The caller reports `message` and aborts.
    Failed { message: String },
}

/// Partial application state returned by non-`Failed` init variants.
/// The caller uses these fields to prompt for a passphrase and then
/// calls one of the `complete_*` methods.
pub struct InitParts<F: VaultStorageFactory, I: IdentityStore> {
    pub config: AppConfig,
    pub config_path: PathBuf,
    pub identity_path: PathBuf,
    pub registry: VaultRegistry<F::Storage>,
    pub lock_timeout: Option<u64>,
    pub pending_identity: Option<AgeIdentity>,
    pub identity_store: I,
}

/// Fully-bootstrapped application state.
pub struct App<F: VaultStorageFactory, I: IdentityStore> {
    pub config: AppConfig,
    pub identity: Option<AgeIdentity>,
    pub registry: VaultRegistry<F::Storage>,
    pub is_locked: bool,
    pub lock_timeout: Option<u64>,
    pub identity_store: I,
}

impl<F: VaultStorageFactory, I: IdentityStore> App<F, I> {
    /// Bootstrap the application with injected components.
    ///
    /// Returns an `InitResult` that the caller must handle:
    ///
    /// | Variant | Meaning | Next step |
    /// |---------|---------|-----------|
    /// | `NeedsPassphrase` | First run — no identity file | Prompt for passphrase, call `complete_first_run` |
    /// | `Locked` | Encrypted identity, no identity held | Prompt for passphrase, call `complete_unlock` |
    /// | `Failed` | Initialization failed | Report the message and abort |
    pub async fn init_with(factory: F, identity_store: I) -> InitResult<F, I> {
        let config_path = AppConfig::default_path();
        let config = AppConfig::load(&config_path).unwrap_or_else(|_| AppConfig::default());
        let identity_path = config.effective_identity_path();

        if !identity_path.exists() {
            return Self::first_run(&config_path, factory, identity_store).await;
        }

        Self::load_existing(config, &config_path, factory, identity_store).await
    }

    async fn first_run(config_path: &Path, factory: F, identity_store: I) -> InitResult<F, I> {
        let config = AppConfig::default();

        // 1. Generate identity
        let identity = AgeIdentity::generate();
        let identity_path = config.effective_identity_path();

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
            sync_target: None,
            recipients: vec![recipient],
            lock_timeout: None,
            auto_sync: false,
        };

        // 4. Open storage through the injected factory, persist the vault
        let storage = match factory.open(&db_path).await {
            Ok(storage) => storage,
            Err(e) => {
                eprintln!("error: failed to open database: {e}");
                let fallback = match factory.open_in_memory() {
                    Ok(storage) => storage,
                    Err(fallback_error) => {
                        return InitResult::Failed {
                            message: format!(
                                "failed to open database: {e}; in-memory fallback failed: {fallback_error}"
                            ),
                        };
                    }
                };
                return InitResult::NeedsPassphrase(InitParts {
                    config,
                    config_path: config_path.to_path_buf(),
                    identity_path: identity_path.clone(),
                    registry: VaultRegistry::new(fallback),
                    lock_timeout: None,
                    pending_identity: Some(identity),
                    identity_store,
                });
            }
        };
        let registry = VaultRegistry::new(storage);
        if let Err(e) = registry.add_vault(&vault).await {
            eprintln!("error: failed to create default vault: {e}");
        }

        InitResult::NeedsPassphrase(InitParts {
            config,
            config_path: config_path.to_path_buf(),
            identity_path,
            registry,
            lock_timeout: None,
            pending_identity: Some(identity),
            identity_store,
        })
    }

    async fn load_existing(
        config: AppConfig,
        config_path: &Path,
        factory: F,
        identity_store: I,
    ) -> InitResult<F, I> {
        let identity_path = config.effective_identity_path();
        let db_path = config.effective_db_path();

        let storage = match factory.open(&db_path).await {
            Ok(storage) => storage,
            Err(e) => {
                eprintln!("error: failed to open database: {e}");
                match factory.open_in_memory() {
                    Ok(storage) => storage,
                    Err(fallback_error) => {
                        return InitResult::Failed {
                            message: format!(
                                "failed to open database: {e}; in-memory fallback failed: {fallback_error}"
                            ),
                        };
                    }
                }
            }
        };
        let registry = VaultRegistry::new(storage);

        // Query lock_timeout from the default vault
        let lock_timeout = registry
            .default_vault()
            .await
            .ok()
            .flatten()
            .and_then(|v| v.lock_timeout);

        // The identity file exists, so the vault is initialized but nothing is
        // held: the caller derives the identity with a passphrase.
        InitResult::Locked(InitParts {
            config,
            config_path: config_path.to_path_buf(),
            identity_path,
            registry,
            lock_timeout,
            pending_identity: None,
            identity_store,
        })
    }

    /// Complete first-run setup: encrypt the identity with the passphrase, save
    /// the config, and return a ready `App` holding the identity in memory.
    pub async fn complete_first_run(
        parts: InitParts<F, I>,
        passphrase: &SecretString,
    ) -> Result<Self> {
        let identity = parts
            .pending_identity
            .ok_or(Error::Session(crate::error::SessionError::Locked))?;

        // Write encrypted identity through the injected store
        parts
            .identity_store
            .write_encrypted(&identity, passphrase, &parts.identity_path)?;

        // Save config only after successful first-run completion
        if let Err(e) = parts.config.save(&parts.config_path) {
            eprintln!("warning: failed to save config: {e}");
        }

        Ok(App {
            config: parts.config,
            identity: Some(identity),
            registry: parts.registry,
            is_locked: false,
            lock_timeout: parts.lock_timeout,
            identity_store: parts.identity_store,
        })
    }

    /// Complete unlock: decrypt the identity with the passphrase and hold it in
    /// memory for the lifetime of this `App`.
    pub async fn complete_unlock(
        parts: InitParts<F, I>,
        passphrase: &SecretString,
    ) -> Result<Self> {
        let identity = parts
            .identity_store
            .read_encrypted(&parts.identity_path, passphrase)?;

        Ok(App {
            config: parts.config,
            identity: Some(identity),
            registry: parts.registry,
            is_locked: false,
            lock_timeout: parts.lock_timeout,
            identity_store: parts.identity_store,
        })
    }

    /// Build the app in its locked state, without asking for a passphrase.
    ///
    /// The registry is already open, so a caller can render a locked shell. Nothing is held: the
    /// identity stays out of memory until a daemon-held lock derives it, which is the same step
    /// the CLI's `complete_unlock` performs.
    pub fn pending(parts: InitParts<F, I>) -> Self {
        Self {
            config: parts.config,
            identity: None,
            registry: parts.registry,
            is_locked: true,
            lock_timeout: parts.lock_timeout,
            identity_store: parts.identity_store,
        }
    }
}

impl App<SqliteStorageFactory, EncryptedIdentityFile> {
    /// Bootstrap the application with SQLite storage and the encrypted identity
    /// file.
    ///
    /// This is the production composition root; tests call [`App::init_with`]
    /// with injected components.
    pub async fn init() -> InitResult<SqliteStorageFactory, EncryptedIdentityFile> {
        Self::init_with(SqliteStorageFactory, EncryptedIdentityFile).await
    }
}

// region:    --- Tests
#[cfg(all(test, feature = "integration"))]
#[path = "../test/app.rs"]
mod app_test;
// endregion: --- Tests
