use bogita_core::agent::{KeyRing, SharedKeyRing, VaultLock, VaultService};
use bogita_core::crypto::AgeCrypto;
use bogita_core::domain::{AgeIdentity, FieldType, FieldValue, SecretString};
use bogita_core::ports::EncryptedIdentityFile;
use bogita_core::storage::identity::write_identity_encrypted;
use bogita_core::storage::sqlite::SqliteStorage;
use bogita_core::test_support::{test_entry, test_field, test_vault_with};
use bogita_core::vault::registry::VaultRegistry;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;
pub(super) type TestService =
    VaultLock<EncryptedIdentityFile, VaultRegistry<SqliteStorage<AgeCrypto>>>;

/// The passphrase the unlockable fixture encrypts its identity file with.
pub(super) const UNLOCK_PASSPHRASE: &str = "test passphrase";

/// How a fixture boot finds the vault.
enum Boot {
    /// Identity held in memory, no identity file: the staged boot.
    Unlocked,
    /// No identity and no identity file: every passphrase is refused.
    Locked,
    /// No identity, identity file on disk: the passphrase opens it.
    Unlockable,
}

/// Build the TUI's composed vault service over a throwaway database.
async fn make_service(boot: Boot) -> Result<TestService> {
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

    let identity_path = dir.path().join("identity.age");
    if matches!(boot, Boot::Unlockable) {
        write_identity_encrypted(
            &identity,
            &SecretString::from(UNLOCK_PASSPHRASE),
            &identity_path,
        )?;
        let entry = test_entry(vault.id, "GitHub", vec![]);
        registry
            .vault_service_for(&vault, identity.clone())
            .save_entry(&entry)
            .await?;
    }

    // Leak the tempdir so the DB file lives for the duration of the test.
    std::mem::forget(dir);

    let ring: SharedKeyRing = Arc::new(Mutex::new(KeyRing::new()));
    let service = VaultLock::new(identity_path, EncryptedIdentityFile, registry, ring);
    Ok(if matches!(boot, Boot::Unlocked) {
        service.with_identity(identity)
    } else {
        service
    })
}

/// Build the composed vault service, unlocked.
pub(super) async fn make_app() -> Result<TestService> {
    make_service(Boot::Unlocked).await
}

/// Build the same service in its locked state, the way a TUI boot finds it.
pub(super) async fn make_locked_app() -> Result<TestService> {
    make_service(Boot::Locked).await
}

/// Build a locked service whose identity file `UNLOCK_PASSPHRASE` opens.
pub(super) async fn make_unlockable_app() -> Result<TestService> {
    make_service(Boot::Unlockable).await
}

/// Build the service seeded with one entry in the Personal vault.
pub(super) async fn make_app_with_entry() -> Result<TestService> {
    // -- Setup & Fixtures
    let service = make_app().await?;
    let vaults = service.list_vaults().await?;
    let vault = vaults.first().ok_or("should have a vault")?;
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
    service.save_entry(&entry).await?;

    // -- Check
    Ok(service)
}
