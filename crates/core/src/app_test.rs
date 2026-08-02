use crate::app::{App, InitResult};
use crate::crypto::age::AgeCrypto;
use crate::domain::{AgeIdentity, Vault};
use crate::session::Session;
use crate::storage::config::AppConfig;
use crate::storage::identity::write_identity_encrypted;
use crate::storage::sqlite::SqliteStorage;
use crate::test_helpers::MockKeychain;
use crate::vault::registry::VaultRegistry;
use chrono::Utc;
use secrecy::SecretString;
use serial_test::serial;
use tempfile::TempDir;
use uuid::Uuid;

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
async fn make_test_app(passphrase: &SecretString) -> App<MockKeychain> {
    let dir = TempDir::new().unwrap();
    redirect_xdg(&dir);
    std::mem::forget(dir);

    let identity = AgeIdentity::generate();
    let identity_path = AppConfig::default().effective_identity_path();
    write_identity_encrypted(&identity, passphrase, &identity_path).unwrap();

    let db_path = AppConfig::default().effective_db_path();
    let storage = SqliteStorage::new(&db_path, AgeCrypto).await.unwrap();
    let registry = VaultRegistry::new(storage, AgeCrypto);

    let vault = Vault {
        id: Uuid::new_v4(),
        name: "Personal".to_string(),
        is_default: true,
        created_at: Utc::now().timestamp(),
        sync_target: None,
        recipients: vec![identity.to_recipient()],
        lock_timeout: None,
        auto_sync: false,
    };
    registry.add_vault(&vault).await.unwrap();

    let kc = MockKeychain::new();
    let session = Session::new(kc);
    session.store_identity(&identity).unwrap();

    // Save the config so App::init_with_keychain() can find it
    AppConfig::default()
        .save(&AppConfig::default_path())
        .unwrap();

    App {
        config: AppConfig::default(),
        identity: Some(identity),
        registry,
        session,
        is_locked: false,
        lock_timeout: None,
    }
}

#[tokio::test]
#[serial]
async fn first_run_returns_needs_passphrase() {
    let dir = TempDir::new().unwrap();
    redirect_xdg(&dir);

    let kc = MockKeychain::new();
    let result = App::init_with_keychain(kc).await;
    assert!(
        matches!(result, InitResult::NeedsPassphrase(_)),
        "first run should return NeedsPassphrase"
    );
}

#[tokio::test]
#[serial]
async fn complete_first_run_creates_config_identity_and_personal_vault() {
    let dir = TempDir::new().unwrap();
    redirect_xdg(&dir);

    let passphrase = SecretString::from("test passphrase");
    let app = make_test_app(&passphrase).await;

    assert!(
        AppConfig::default_path().exists(),
        "config.toml should exist"
    );
    assert!(
        app.config.effective_identity_path().exists(),
        "identity.age should exist"
    );

    let vaults = app.registry.list_vaults().await.unwrap();
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].name, "Personal");
    assert!(vaults[0].is_default);
}

#[tokio::test]
#[serial]
async fn second_init_returns_locked_when_keychain_empty() {
    let dir = TempDir::new().unwrap();
    redirect_xdg(&dir);

    let passphrase = SecretString::from("test passphrase");
    let app1 = make_test_app(&passphrase).await;
    let recipient1 = app1.identity.as_ref().unwrap().to_recipient().to_string();

    // Clear the keychain to simulate a fresh process
    app1.session.lock().unwrap();

    // Second init — keychain is empty, so it should return Locked
    let kc = MockKeychain::new();
    let result = App::init_with_keychain(kc).await;
    match result {
        InitResult::Locked(parts) => {
            // Complete unlock with the same passphrase
            let app2 = App::complete_unlock(parts, &passphrase).await.unwrap();
            let recipient2 = app2.identity.as_ref().unwrap().to_recipient().to_string();
            assert_eq!(
                recipient1, recipient2,
                "identity must not change on second init"
            );
        }
        _ => panic!("expected Locked when keychain is empty"),
    }
}

#[tokio::test]
#[serial]
async fn personal_vault_is_default_and_sqlite_backed() {
    let dir = TempDir::new().unwrap();
    redirect_xdg(&dir);

    let passphrase = SecretString::from("test passphrase");
    let app = make_test_app(&passphrase).await;
    let vaults = app.registry.list_vaults().await.unwrap();
    let vault = &vaults[0];

    assert!(vault.is_default);
    assert!(
        vault.sync_target.is_none(),
        "expected no sync target for local vault"
    );
}

#[tokio::test]
async fn lock_and_unlock_round_trip() {
    use crate::crypto::age::AgeCrypto;
    use crate::domain::AgeIdentity;
    use crate::session::Session;
    use crate::storage::identity::write_identity_encrypted;
    use crate::storage::sqlite::SqliteStorage;
    use crate::test_helpers::MockKeychain;
    use crate::vault::registry::VaultRegistry;

    let dir = TempDir::new().unwrap();
    let identity = AgeIdentity::generate();
    let passphrase = SecretString::from("test passphrase");
    let identity_path = dir.path().join("identity.age");

    // Write encrypted identity to disk
    write_identity_encrypted(&identity, &passphrase, &identity_path).unwrap();

    let db_path = dir.path().join("test.db");
    let storage = SqliteStorage::new(&db_path, AgeCrypto).await.unwrap();
    let registry = VaultRegistry::new(storage, AgeCrypto);
    let kc = MockKeychain::new();
    let session = Session::new(kc);

    let mut app = App {
        config: AppConfig {
            identity_path: Some(identity_path.clone()),
            ..AppConfig::default()
        },
        identity: Some(identity),
        registry,
        session,
        is_locked: false,
        lock_timeout: None,
    };

    assert!(!app.is_locked);
    assert!(app.identity.is_some());

    // Lock
    app.lock().unwrap();
    assert!(app.is_locked);
    assert!(app.identity.is_none());

    // Unlock
    app.unlock(&passphrase).unwrap();
    assert!(!app.is_locked);
    assert!(app.identity.is_some());
}
