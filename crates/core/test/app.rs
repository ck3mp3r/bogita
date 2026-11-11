use crate::app::InitParts;
use crate::app::{App, InitResult};
use crate::crypto::age::AgeCrypto;
use crate::domain::AgeIdentity;
use crate::ports::{EncryptedIdentityFile, SqliteStorageFactory, VaultStorageFactory};
use crate::storage::config::AppConfig;
use crate::storage::identity::write_identity_encrypted;
use crate::storage::sqlite::SqliteStorage;
use crate::test_support::test_vault_with;
use crate::vault::registry::VaultRegistry;
use secrecy::SecretString;
use serial_test::serial;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;
type TestApp = App<SqliteStorageFactory, EncryptedIdentityFile>;

/// Redirect HOME and XDG dirs to `dir` for the duration of the test.
/// Returns the TempDir so it stays alive for the test body.
fn redirect_xdg(dir: &TempDir) {
    std::env::set_var("HOME", dir.path());
    std::env::set_var("XDG_DATA_HOME", dir.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.path().join("config"));
}

/// Build a fully-initialized App for testing, bypassing App::init().
/// Creates an encrypted identity and a Personal vault.
async fn make_test_app(passphrase: &SecretString) -> Result<TestApp> {
    // -- Setup & Fixtures
    let dir = TempDir::new()?;
    redirect_xdg(&dir);
    std::mem::forget(dir);

    let identity = AgeIdentity::generate();
    let identity_path = AppConfig::default().effective_identity_path();
    write_identity_encrypted(&identity, passphrase, &identity_path)?;

    let db_path = AppConfig::default().effective_db_path();
    let storage = SqliteStorage::new(&db_path, AgeCrypto).await?;
    let registry = VaultRegistry::new(storage);

    let vault = test_vault_with(
        Uuid::new_v4(),
        "Personal",
        true,
        vec![identity.to_recipient()],
    );
    registry.add_vault(&vault).await?;

    // Save the config so App::init_with() can find it
    AppConfig::default().save(&AppConfig::default_path())?;

    // -- Exec & Check
    Ok(App {
        config: AppConfig::default(),
        identity: Some(identity),
        registry,
        is_locked: false,
        lock_timeout: None,
        identity_store: EncryptedIdentityFile,
    })
}

#[tokio::test]
#[serial]
async fn first_run_returns_needs_passphrase() -> Result<()> {
    // -- Setup & Fixtures
    let dir = TempDir::new()?;
    redirect_xdg(&dir);

    // -- Exec
    let result = App::init_with(SqliteStorageFactory, EncryptedIdentityFile).await;

    // -- Check
    assert!(
        matches!(result, InitResult::NeedsPassphrase(_)),
        "first run should return NeedsPassphrase"
    );
    Ok(())
}

#[tokio::test]
#[serial]
async fn complete_first_run_creates_config_identity_and_personal_vault() -> Result<()> {
    // -- Setup & Fixtures
    let dir = TempDir::new()?;
    redirect_xdg(&dir);

    // -- Exec
    let passphrase = SecretString::from("test passphrase");
    let app = make_test_app(&passphrase).await?;

    // -- Check
    assert!(
        AppConfig::default_path().exists(),
        "config.toml should exist"
    );
    assert!(
        app.config.effective_identity_path().exists(),
        "identity.age should exist"
    );
    let vaults = app.registry.list_vaults().await?;
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].name, "Personal");
    assert!(vaults[0].is_default);
    Ok(())
}

#[tokio::test]
#[serial]
async fn second_init_returns_locked_and_unlock_keeps_the_identity() -> Result<()> {
    // -- Setup & Fixtures
    let dir = TempDir::new()?;
    redirect_xdg(&dir);

    let passphrase = SecretString::from("test passphrase");
    let app1 = make_test_app(&passphrase).await?;
    let recipient1 = app1
        .identity
        .as_ref()
        .ok_or("should have identity")?
        .to_recipient()
        .to_string();
    drop(app1);

    // -- Exec: a second boot in a fresh process state
    let result = App::init_with(SqliteStorageFactory, EncryptedIdentityFile).await;

    // -- Check
    match result {
        InitResult::Locked(parts) => {
            // Complete unlock with the same passphrase
            let app2 = App::complete_unlock(parts, &passphrase).await?;
            let recipient2 = app2
                .identity
                .as_ref()
                .ok_or("should have identity")?
                .to_recipient()
                .to_string();
            assert_eq!(
                recipient1, recipient2,
                "identity must not change on second init"
            );
        }
        _ => panic!("expected Locked on a boot with an existing identity file"),
    }
    Ok(())
}

#[tokio::test]
#[serial]
async fn personal_vault_is_default_and_sqlite_backed() -> Result<()> {
    // -- Setup & Fixtures
    let dir = TempDir::new()?;
    redirect_xdg(&dir);
    let passphrase = SecretString::from("test passphrase");

    // -- Exec
    let app = make_test_app(&passphrase).await?;
    let vaults = app.registry.list_vaults().await?;
    let vault = &vaults[0];

    // -- Check
    assert!(vault.is_default);
    assert!(
        vault.sync_target.is_none(),
        "expected no sync target for local vault"
    );
    Ok(())
}

// ── pending boot ────────────────────────────────────────────────────────────

#[tokio::test]
async fn pending_boots_locked_and_holds_nothing() -> Result<()> {
    // -- Setup & Fixtures: an encrypted identity on disk
    let dir = TempDir::new()?;
    let identity = AgeIdentity::generate();
    let passphrase = SecretString::from("test passphrase");
    let identity_path = dir.path().join("identity.age");
    write_identity_encrypted(&identity, &passphrase, &identity_path)?;
    let db_path = dir.path().join("pending.db");
    let storage = SqliteStorage::new(&db_path, AgeCrypto).await?;
    let parts = InitParts {
        config: AppConfig {
            identity_path: Some(identity_path.clone()),
            ..AppConfig::default()
        },
        config_path: dir.path().join("config.toml"),
        identity_path,
        registry: VaultRegistry::new(storage),
        lock_timeout: None,
        pending_identity: None,
        identity_store: EncryptedIdentityFile,
    };

    // -- Exec
    let app: TestApp = App::pending(parts);

    // -- Check: locked and holding nothing, even with an identity file on disk
    assert!(app.is_locked, "a pending boot is locked");
    assert!(app.identity.is_none(), "a pending boot holds no identity");
    Ok(())
}

// ── injected storage factory ─────────────────────────────────────────────────

struct FailingFactory;

#[async_trait::async_trait]
impl VaultStorageFactory for FailingFactory {
    type Storage = SqliteStorage<AgeCrypto>;

    async fn open(&self, _db_path: &Path) -> crate::error::Result<Self::Storage> {
        Err(crate::error::DbError::ConnectionFailed("injected failure".to_string()).into())
    }

    fn open_in_memory(&self) -> crate::error::Result<Self::Storage> {
        Err(crate::error::DbError::ConnectionFailed("injected fallback failure".to_string()).into())
    }
}

#[derive(Clone, Default)]
struct InMemoryFactory {
    opened: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl VaultStorageFactory for InMemoryFactory {
    type Storage = SqliteStorage<AgeCrypto>;

    async fn open(&self, _db_path: &Path) -> crate::error::Result<Self::Storage> {
        self.opened.store(true, Ordering::SeqCst);
        SqliteStorage::new_in_memory(AgeCrypto)
    }

    fn open_in_memory(&self) -> crate::error::Result<Self::Storage> {
        SqliteStorage::new_in_memory(AgeCrypto)
    }
}

#[tokio::test]
#[serial]
async fn injected_factory_failure_surfaces_init_result_failed() -> Result<()> {
    // -- Setup & Fixtures
    let dir = TempDir::new()?;
    redirect_xdg(&dir);

    // -- Exec
    let result = App::init_with(FailingFactory, EncryptedIdentityFile).await;

    // -- Check
    match result {
        InitResult::Failed { message } => assert!(
            message.contains("injected failure"),
            "expected the factory error in the message, got: {message}"
        ),
        _ => panic!("expected InitResult::Failed when the factory cannot open storage"),
    }
    Ok(())
}

#[tokio::test]
#[serial]
async fn injected_in_memory_factory_opens_storage_and_boots_locked() -> Result<()> {
    // -- Setup & Fixtures
    let dir = TempDir::new()?;
    redirect_xdg(&dir);
    let passphrase = SecretString::from("test passphrase");
    let identity = AgeIdentity::generate();
    let identity_path = AppConfig::default().effective_identity_path();
    write_identity_encrypted(&identity, &passphrase, &identity_path)?;
    AppConfig::default().save(&AppConfig::default_path())?;

    // -- Exec
    let factory = InMemoryFactory::default();
    let opened = factory.opened.clone();
    let result = App::init_with(factory, EncryptedIdentityFile).await;

    // -- Check
    assert!(
        opened.load(Ordering::SeqCst),
        "init must open storage through the injected factory"
    );
    match result {
        InitResult::Locked(parts) => assert!(
            parts.pending_identity.is_none(),
            "an existing vault boots locked, holding nothing"
        ),
        _ => panic!("expected Locked with the injected in-memory factory"),
    }
    Ok(())
}
