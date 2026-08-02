//! Application bootstrap — first-run init and subsequent startup.

use crate::crypto::age::AgeCrypto;
use crate::domain::{AgeIdentity, Vault};
use crate::error::{Error, Result};
use crate::ports::KeychainStore;
use crate::session::Session;
use crate::storage::config::AppConfig;
use crate::storage::identity::{read_identity_encrypted, write_identity_encrypted};
use crate::storage::keychain::KeychainAdapter;
use crate::storage::sqlite::SqliteStorage;
use crate::vault::registry::VaultRegistry;
use chrono::Utc;
use secrecy::SecretString;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Result of [`App::init()`] — the caller must handle each variant.
pub enum InitResult<K: KeychainStore> {
    /// App is fully ready — identity is loaded and unlocked.
    Ready(App<K>),
    /// First run: no identity file exists. Caller must prompt for a passphrase
    /// and call [`App::complete_first_run`].
    NeedsPassphrase(InitParts<K>),
    /// Encrypted identity found but keychain is empty (locked).
    /// Caller must prompt for a passphrase and call [`App::complete_unlock`].
    Locked(InitParts<K>),
}

/// Partial application state returned by non-`Ready` init variants.
/// The caller uses these fields to prompt for a passphrase and then
/// calls one of the `complete_*` methods.
pub struct InitParts<K: KeychainStore> {
    pub config: AppConfig,
    pub config_path: PathBuf,
    pub identity_path: PathBuf,
    pub session: Session<K>,
    pub registry: VaultRegistry<SqliteStorage<AgeCrypto>, AgeCrypto>,
    pub lock_timeout: Option<u64>,
    pub pending_identity: Option<AgeIdentity>,
}

/// Fully-bootstrapped application state.
pub struct App<K: KeychainStore> {
    pub config: AppConfig,
    pub identity: Option<AgeIdentity>,
    pub registry: VaultRegistry<SqliteStorage<AgeCrypto>, AgeCrypto>,
    pub session: Session<K>,
    pub is_locked: bool,
    pub lock_timeout: Option<u64>,
}

impl<K: KeychainStore> App<K> {
    /// Bootstrap the application with a specific keychain implementation.
    ///
    /// Returns an `InitResult` that the caller must handle:
    ///
    /// | Variant | Meaning | Next step |
    /// |---------|---------|-----------|
    /// | `Ready` | Identity loaded and unlocked | Use the app directly |
    /// | `NeedsPassphrase` | First run — no identity file | Prompt for passphrase, call `complete_first_run` |
    /// | `Locked` | Encrypted identity, empty keychain | Prompt for passphrase, call `complete_unlock` |
    pub async fn init_with_keychain(keychain: K) -> InitResult<K> {
        let config_path = AppConfig::default_path();
        let config = AppConfig::load(&config_path).unwrap_or_else(|_| AppConfig::default());
        let identity_path = config.effective_identity_path();

        if !identity_path.exists() {
            return Self::first_run_with_keychain(&config_path, keychain).await;
        }

        Self::load_existing_with_keychain(config, &config_path, keychain).await
    }

    async fn first_run_with_keychain(config_path: &Path, keychain: K) -> InitResult<K> {
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

        // 4. Open SQLite storage and registry, persist the vault
        let crypto = AgeCrypto;
        let storage = match SqliteStorage::new(&db_path, crypto).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: failed to open database: {e}");
                return InitResult::NeedsPassphrase(InitParts {
                    config,
                    config_path: config_path.to_path_buf(),
                    identity_path: identity_path.clone(),
                    session: Session::new(keychain),
                    registry: VaultRegistry::new(
                        SqliteStorage::new_in_memory(AgeCrypto).unwrap_or_else(|e| {
                            panic!("failed to create in-memory SQLite pool: {e}")
                        }),
                        AgeCrypto,
                    ),
                    lock_timeout: None,
                    pending_identity: Some(identity),
                });
            }
        };
        let registry = VaultRegistry::new(storage, AgeCrypto);
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
        })
    }

    async fn load_existing_with_keychain(
        config: AppConfig,
        config_path: &Path,
        keychain: K,
    ) -> InitResult<K> {
        let identity_path = config.effective_identity_path();
        let db_path = config.effective_db_path();
        let crypto = AgeCrypto;

        let storage = match SqliteStorage::new(&db_path, crypto).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: failed to open database: {e}");
                let fallback = SqliteStorage::new_in_memory(AgeCrypto)
                    .unwrap_or_else(|e| panic!("failed to create in-memory SQLite pool: {e}"));
                let registry = VaultRegistry::new(fallback, AgeCrypto);
                let session = Session::new(keychain);
                return InitResult::NeedsPassphrase(InitParts {
                    config,
                    config_path: config_path.to_path_buf(),
                    identity_path,
                    session,
                    registry,
                    lock_timeout: None,
                    pending_identity: None,
                });
            }
        };
        let registry = VaultRegistry::new(storage, AgeCrypto);

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
                })
            }
        }
    }

    /// Complete first-run setup: encrypt the identity with the passphrase,
    /// store it in the keychain, save the config, and return a ready `App`.
    pub async fn complete_first_run(
        parts: InitParts<K>,
        passphrase: &SecretString,
    ) -> Result<Self> {
        let identity = parts
            .pending_identity
            .ok_or(Error::Session(crate::error::SessionError::Locked))?;

        // Write encrypted identity
        write_identity_encrypted(&identity, passphrase, &parts.identity_path)?;

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
        })
    }

    /// Complete unlock: decrypt the identity with the passphrase and store
    /// it in the keychain.
    pub async fn complete_unlock(parts: InitParts<K>, passphrase: &SecretString) -> Result<Self> {
        let identity = read_identity_encrypted(&parts.identity_path, passphrase)?;

        // Store in keychain
        parts.session.store_identity(&identity)?;

        Ok(App {
            config: parts.config,
            identity: Some(identity),
            registry: parts.registry,
            session: parts.session,
            is_locked: false,
            lock_timeout: parts.lock_timeout,
        })
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
        let identity = read_identity_encrypted(&self.config.effective_identity_path(), passphrase)?;
        self.session.store_identity(&identity)?;
        self.identity = Some(identity);
        self.is_locked = false;
        Ok(())
    }
}

impl App<KeychainAdapter> {
    /// Bootstrap the application with the real OS keychain.
    ///
    /// Convenience wrapper around [`App::init_with_keychain`] for production use.
    /// Tests should call [`App::init_with_keychain`] with a [`MockKeychain`].
    ///
    /// [`MockKeychain`]: crate::test_helpers::MockKeychain
    pub async fn init() -> InitResult<KeychainAdapter> {
        Self::init_with_keychain(KeychainAdapter::new()).await
    }
}
