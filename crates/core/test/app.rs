use crate::app::InitParts;
use crate::app::{App, InitResult};
use crate::crypto::age::AgeCrypto;
use crate::domain::AgeIdentity;
use crate::ports::{EncryptedIdentityFile, SqliteStorageFactory, VaultStorageFactory};
use crate::session::Session;
use crate::storage::config::AppConfig;
use crate::storage::identity::write_identity_encrypted;
use crate::storage::sqlite::SqliteStorage;
use crate::test_support::{test_vault_with, MockKeychain};
use crate::vault::registry::VaultRegistry;
use secrecy::SecretString;
use serial_test::serial;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;
type TestApp = App<MockKeychain, SqliteStorageFactory, EncryptedIdentityFile>;

/// Redirect HOME and XDG dirs to `dir` for the duration of the test.
/// Returns the TempDir so it stays alive for the test body.
fn redirect_xdg(dir: &TempDir) {
    std::env::set_var("HOME", dir.path());
    std::env::set_var("XDG_DATA_HOME", dir.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.path().join("config"));
}

/// Build a fully-initialized App for testing, bypassing App::init().
/// Creates an encrypted identity, a Personal vault, and stores the identity
/// in the keychain.
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

    let kc = MockKeychain::new();
    let session = Session::new(kc);
    session.store_identity(&identity)?;

    // Save the config so App::init_with_keychain() can find it
    AppConfig::default().save(&AppConfig::default_path())?;

    // -- Exec & Check
    Ok(App {
        config: AppConfig::default(),
        identity: Some(identity),
        registry,
        session,
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
    let kc = MockKeychain::new();
    let result = App::init_with(kc, SqliteStorageFactory, EncryptedIdentityFile).await;

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
async fn second_init_returns_locked_when_keychain_empty() -> Result<()> {
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

    // Clear the keychain to simulate a fresh process
    app1.session.lock()?;

    // -- Exec
    let kc = MockKeychain::new();
    let result = App::init_with(kc, SqliteStorageFactory, EncryptedIdentityFile).await;

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
        _ => panic!("expected Locked when keychain is empty"),
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

#[tokio::test]
async fn lock_and_unlock_round_trip() -> Result<()> {
    // -- Setup & Fixtures
    let dir = TempDir::new()?;
    let identity = AgeIdentity::generate();
    let passphrase = SecretString::from("test passphrase");
    let identity_path = dir.path().join("identity.age");
    write_identity_encrypted(&identity, &passphrase, &identity_path)?;

    let db_path = dir.path().join("test.db");
    let storage = SqliteStorage::new(&db_path, AgeCrypto).await?;
    let registry = VaultRegistry::new(storage);
    let kc = MockKeychain::new();
    let session = Session::new(kc);
    let mut app: TestApp = App {
        config: AppConfig {
            identity_path: Some(identity_path.clone()),
            ..AppConfig::default()
        },
        identity: Some(identity),
        registry,
        session,
        is_locked: false,
        lock_timeout: None,
        identity_store: EncryptedIdentityFile,
    };

    // -- Exec
    app.lock()?;
    let locked_state = (app.is_locked, app.identity.is_none());
    app.unlock(&passphrase)?;

    // -- Check
    assert_eq!(locked_state, (true, true));
    assert!(!app.is_locked);
    assert!(app.identity.is_some());
    Ok(())
}

// ── pending boot ────────────────────────────────────────────────────────────

#[tokio::test]
async fn pending_boots_locked_and_stages_nothing_until_unlock() -> Result<()> {
    // -- Setup & Fixtures: an encrypted identity on disk, an empty keychain
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
        session: Session::new(MockKeychain::new()),
        registry: VaultRegistry::new(storage),
        lock_timeout: None,
        pending_identity: None,
        identity_store: EncryptedIdentityFile,
    };

    // -- Exec
    let mut app: TestApp = App::pending(parts);

    // -- Check: locked, no identity, and the keychain stays empty
    assert!(app.is_locked, "a pending boot is locked");
    assert!(app.identity.is_none(), "a pending boot holds no identity");
    assert!(
        app.session.get_identity()?.is_none(),
        "a pending boot must not stage the identity in the keychain"
    );

    // -- Exec: the in-TUI unlock path
    app.unlock(&passphrase)?;

    // -- Check: staging happens at unlock, not before
    assert!(!app.is_locked);
    assert!(app.identity.is_some());
    assert!(
        app.session.get_identity()?.is_some(),
        "unlock stages the identity for the agent"
    );
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
    let result = App::init_with(MockKeychain::new(), FailingFactory, EncryptedIdentityFile).await;

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
async fn injected_in_memory_factory_boots_ready_without_disk_storage() -> Result<()> {
    // -- Setup & Fixtures
    let dir = TempDir::new()?;
    redirect_xdg(&dir);
    let passphrase = SecretString::from("test passphrase");
    let identity = AgeIdentity::generate();
    let identity_path = AppConfig::default().effective_identity_path();
    write_identity_encrypted(&identity, &passphrase, &identity_path)?;
    AppConfig::default().save(&AppConfig::default_path())?;

    let kc = MockKeychain::new();
    {
        use crate::ports::KeychainStore;
        use secrecy::ExposeSecret;
        kc.store_identity(identity.to_secret_string().expose_secret())?;
    }

    // -- Exec
    let factory = InMemoryFactory::default();
    let opened = factory.opened.clone();
    let result = App::init_with(kc, factory, EncryptedIdentityFile).await;

    // -- Check
    assert!(
        opened.load(Ordering::SeqCst),
        "init must open storage through the injected factory"
    );
    match result {
        InitResult::Ready(app) => assert!(app.identity.is_some()),
        _ => panic!("expected Ready with the injected in-memory factory"),
    }
    Ok(())
}
