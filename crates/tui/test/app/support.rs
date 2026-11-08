use bogita_core::app::App;
use bogita_core::crypto::AgeCrypto;
use bogita_core::domain::{AgeIdentity, FieldType, FieldValue};
use bogita_core::ports::{EncryptedIdentityFile, SqliteStorageFactory};
use bogita_core::session::Session;
use bogita_core::storage::config::AppConfig;
use bogita_core::storage::sqlite::SqliteStorage;
use bogita_core::test_support::{test_entry, test_field, test_vault_with, MockKeychain};
use bogita_core::vault::registry::VaultRegistry;
use uuid::Uuid;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;
type TestApp = App<MockKeychain, SqliteStorageFactory, EncryptedIdentityFile>;

/// Build a minimal App for testing without touching the filesystem.
///
/// If `with_entry` is true, seeds the Personal vault with one entry.
pub(super) async fn make_app() -> Result<TestApp> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("tui_test.db");
    let identity = AgeIdentity::generate();
    let storage = SqliteStorage::new(&db_path, AgeCrypto).await?;
    let registry = VaultRegistry::new(storage);
    let vault = test_vault_with(
        Uuid::new_v4(),
        "Personal",
        true,
        vec![identity.to_recipient()],
    );
    registry.add_vault(&vault).await?;

    // Leak the tempdir so the DB file lives for the duration of the test.
    std::mem::forget(dir);

    Ok(App {
        config: AppConfig::default(),
        identity: Some(identity),
        registry,
        session: Session::new(MockKeychain::new()),
        is_locked: false,
        lock_timeout: None,
        identity_store: EncryptedIdentityFile,
    })
}

/// Build an App seeded with one entry in the Personal vault.
pub(super) async fn make_app_with_entry() -> Result<TestApp> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let vaults = app.registry.list_vaults().await?;
    let vault = vaults.first().ok_or("should have a vault")?;
    let identity = app
        .identity
        .as_ref()
        .ok_or("should have an identity")?
        .clone();
    let svc = app.registry.vault_service_for(vault, identity);
    let entry = test_entry(
        vault.id,
        "GitHub",
        vec![test_field(
            "password",
            FieldValue::Hidden("s3cret".to_string()),
            FieldType::Token,
            true,
            0,
        )],
    );

    // -- Exec
    svc.save_entry(&entry).await?;

    // -- Check
    Ok(app)
}
