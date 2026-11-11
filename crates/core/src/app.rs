//! Application bootstrap — first-run init and subsequent startup.

use crate::domain::{AgeIdentity, Entry, Vault};
use crate::error::{Error, Result};
use crate::ports::{
    EncryptedIdentityFile, IdentityStore, KeychainStore, SqliteStorageFactory, VaultStorageFactory,
};
use crate::session::Session;
use crate::storage::config::AppConfig;
use crate::storage::keychain::KeychainAdapter;
use crate::vault::registry::VaultRegistry;
use crate::vault::service::VaultService;
use chrono::Utc;
use secrecy::SecretString;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Fully wired application type used by the binary and the TUI.
pub type DefaultApp = App<KeychainAdapter, SqliteStorageFactory, EncryptedIdentityFile>;

/// Result of [`App::init()`] — the caller must handle each variant.
pub enum InitResult<K: KeychainStore, F: VaultStorageFactory, I: IdentityStore> {
    /// App is fully ready — identity is loaded and unlocked.
    Ready(App<K, F, I>),
    /// First run: no identity file exists. Caller must prompt for a passphrase
    /// and call [`App::complete_first_run`].
    NeedsPassphrase(InitParts<K, F, I>),
    /// Encrypted identity found but keychain is empty (locked).
    /// Caller must prompt for a passphrase and call [`App::complete_unlock`].
    Locked(InitParts<K, F, I>),
    /// Initialization failed and cannot be recovered (for example, no usable database).
    /// The caller reports `message` and aborts.
    Failed { message: String },
}

/// Partial application state returned by non-`Ready` init variants.
/// The caller uses these fields to prompt for a passphrase and then
/// calls one of the `complete_*` methods.
pub struct InitParts<K: KeychainStore, F: VaultStorageFactory, I: IdentityStore> {
    pub config: AppConfig,
    pub config_path: PathBuf,
    pub identity_path: PathBuf,
    pub session: Session<K>,
    pub registry: VaultRegistry<F::Storage>,
    pub lock_timeout: Option<u64>,
    pub pending_identity: Option<AgeIdentity>,
    pub identity_store: I,
}

/// Fully-bootstrapped application state.
pub struct App<K: KeychainStore, F: VaultStorageFactory, I: IdentityStore> {
    pub config: AppConfig,
    pub identity: Option<AgeIdentity>,
    pub registry: VaultRegistry<F::Storage>,
    pub session: Session<K>,
    pub is_locked: bool,
    pub lock_timeout: Option<u64>,
    pub identity_store: I,
}

impl<K: KeychainStore, F: VaultStorageFactory, I: IdentityStore> App<K, F, I> {
    /// Load every entry from every vault.
    ///
    /// Returns an empty list while the vault is locked (no identity loaded).
    pub async fn all_entries(&self) -> Result<Vec<Entry>> {
        let mut all_entries = Vec::new();
        let Some(identity) = self.identity.as_ref() else {
            return Ok(all_entries);
        };

        for vault in self.registry.list_vaults().await? {
            let service = self.registry.vault_service_for(&vault, identity.clone());
            all_entries.extend(service.list_entries(vault.id, None).await?);
        }
        Ok(all_entries)
    }

    /// Build the entry service for one vault.
    ///
    /// Returns `None` while the vault is locked (no identity loaded).
    pub fn service_for(&self, vault: &Vault) -> Option<VaultService<&F::Storage>> {
        let identity = self.identity.as_ref()?;
        Some(self.registry.vault_service_for(vault, identity.clone()))
    }

    /// Bootstrap the application with injected components.
    ///
    /// Returns an `InitResult` that the caller must handle:
    ///
    /// | Variant | Meaning | Next step |
    /// |---------|---------|-----------|
    /// | `Ready` | Identity loaded and unlocked | Use the app directly |
    /// | `NeedsPassphrase` | First run — no identity file | Prompt for passphrase, call `complete_first_run` |
    /// | `Locked` | Encrypted identity, empty keychain | Prompt for passphrase, call `complete_unlock` |
    /// | `Failed` | Initialization failed | Report the message and abort |
    pub async fn init_with(keychain: K, factory: F, identity_store: I) -> InitResult<K, F, I> {
        let config_path = AppConfig::default_path();
        let config = AppConfig::load(&config_path).unwrap_or_else(|_| AppConfig::default());
        let identity_path = config.effective_identity_path();

        if !identity_path.exists() {
            return Self::first_run(&config_path, keychain, factory, identity_store).await;
        }

        Self::load_existing(config, &config_path, keychain, factory, identity_store).await
    }

    async fn first_run(
        config_path: &Path,
        keychain: K,
        factory: F,
        identity_store: I,
    ) -> InitResult<K, F, I> {
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
                    session: Session::new(keychain),
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

        let lock_timeout = None;
        let session = Session::new(keychain);

        InitResult::NeedsPassphrase(InitParts {
            config,
            config_path: config_path.to_path_buf(),
            identity_path,
            session,
            registry,
            lock_timeout,
            pending_identity: Some(identity),
            identity_store,
        })
    }

    async fn load_existing(
        config: AppConfig,
        config_path: &Path,
        keychain: K,
        factory: F,
        identity_store: I,
    ) -> InitResult<K, F, I> {
        let identity_path = config.effective_identity_path();
        let db_path = config.effective_db_path();

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
                let registry = VaultRegistry::new(fallback);
                let session = Session::new(keychain);
                return InitResult::NeedsPassphrase(InitParts {
                    config,
                    config_path: config_path.to_path_buf(),
                    identity_path,
                    session,
                    registry,
                    lock_timeout: None,
                    pending_identity: None,
                    identity_store,
                });
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

        let session = Session::new(keychain);

        // Encrypted identity — check keychain
        match session.get_identity() {
            Ok(Some(identity)) => {
                // Keychain has the identity — ready to go
                InitResult::Ready(App {
                    config,
                    identity: Some(identity),
                    registry,
                    session,
                    is_locked: false,
                    lock_timeout,
                    identity_store,
                })
            }
            Ok(None) => {
                // Keychain is empty — locked
                InitResult::Locked(InitParts {
                    config,
                    config_path: config_path.to_path_buf(),
                    identity_path,
                    session,
                    registry,
                    lock_timeout,
                    pending_identity: None,
                    identity_store,
                })
            }
            Err(e) => {
                eprintln!("warning: keychain error: {e}");
                InitResult::Locked(InitParts {
                    config,
                    config_path: config_path.to_path_buf(),
                    identity_path,
                    session,
                    registry,
                    lock_timeout,
                    pending_identity: None,
                    identity_store,
                })
            }
        }
    }

    /// Complete first-run setup: encrypt the identity with the passphrase,
    /// store it in the keychain, save the config, and return a ready `App`.
    pub async fn complete_first_run(
        parts: InitParts<K, F, I>,
        passphrase: &SecretString,
    ) -> Result<Self> {
        let identity = parts
            .pending_identity
            .ok_or(Error::Session(crate::error::SessionError::Locked))?;

        // Write encrypted identity through the injected store
        parts
            .identity_store
            .write_encrypted(&identity, passphrase, &parts.identity_path)?;

        // Store in keychain
        parts.session.store_identity(&identity)?;

        // Save config only after successful first-run completion
        if let Err(e) = parts.config.save(&parts.config_path) {
            eprintln!("warning: failed to save config: {e}");
        }

        Ok(App {
            config: parts.config,
            identity: Some(identity),
            registry: parts.registry,
            session: parts.session,
            is_locked: false,
            lock_timeout: parts.lock_timeout,
            identity_store: parts.identity_store,
        })
    }

    /// Complete unlock: decrypt the identity with the passphrase and store
    /// it in the keychain.
    pub async fn complete_unlock(
        parts: InitParts<K, F, I>,
        passphrase: &SecretString,
    ) -> Result<Self> {
        let identity = parts
            .identity_store
            .read_encrypted(&parts.identity_path, passphrase)?;

        // Store in keychain
        parts.session.store_identity(&identity)?;

        Ok(App {
            config: parts.config,
            identity: Some(identity),
            registry: parts.registry,
            session: parts.session,
            is_locked: false,
            lock_timeout: parts.lock_timeout,
            identity_store: parts.identity_store,
        })
    }

    /// Build the app in its locked state, without asking for a passphrase.
    ///
    /// The registry is already open, so a caller can render a locked shell and
    /// unlock in place. Nothing is staged: the identity stays out of memory and
    /// out of the keychain until [`Self::unlock`] runs, which is the same
    /// staging step the CLI's `complete_unlock` performs.
    pub fn pending(parts: InitParts<K, F, I>) -> Self {
        Self {
            config: parts.config,
            identity: None,
            registry: parts.registry,
            session: parts.session,
            is_locked: true,
            lock_timeout: parts.lock_timeout,
            identity_store: parts.identity_store,
        }
    }

    /// Lock the app: remove identity from keychain and clear in-memory identity.
    pub fn lock(&mut self) -> Result<()> {
        self.session.lock()?;
        self.identity = None;
        self.is_locked = true;
        Ok(())
    }

    /// Unlock the app: decrypt identity with passphrase and store in keychain.
    pub fn unlock(&mut self, passphrase: &SecretString) -> Result<()> {
        let identity = self
            .identity_store
            .read_encrypted(&self.config.effective_identity_path(), passphrase)?;
        self.session.store_identity(&identity)?;
        self.identity = Some(identity);
        self.is_locked = false;
        Ok(())
    }
}

impl App<KeychainAdapter, SqliteStorageFactory, EncryptedIdentityFile> {
    /// Bootstrap the application with the real OS keychain, SQLite storage and
    /// the encrypted identity file.
    ///
    /// This is the production composition root; tests call [`App::init_with`]
    /// with mock components.
    pub async fn init() -> InitResult<KeychainAdapter, SqliteStorageFactory, EncryptedIdentityFile>
    {
        let keychain = match crate::storage::keychain::platform_adapter() {
            Ok(adapter) => adapter,
            Err(e) => {
                return InitResult::Failed {
                    message: format!("keychain unavailable: {e}"),
                }
            }
        };
        Self::init_with(keychain, SqliteStorageFactory, EncryptedIdentityFile).await
    }
}

// region:    --- Tests
#[cfg(all(test, feature = "integration"))]
#[path = "../test/app.rs"]
mod app_test;
// endregion: --- Tests
