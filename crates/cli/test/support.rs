//! Shared fixtures for bogita-cli handler tests.

use bogita_core::crypto::AgeCrypto;
use bogita_core::domain::{AgeIdentity, Entry, Vault};
use bogita_core::storage::sqlite::SqliteStorage;
use bogita_core::test_support::test_vault_with;
use bogita_core::vault::registry::VaultRegistry;
use uuid::Uuid;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

/// Build a registry with a default `Personal` vault seeded with `entries`.
///
/// Each entry is re-bound to the created vault, so callers pass
/// `Uuid::nil()` as the vault id.
pub async fn make_registry(
    db_path: &std::path::Path,
    entries: &[Entry],
) -> Result<(VaultRegistry<SqliteStorage<AgeCrypto>>, AgeIdentity, Vault)> {
    // -- Setup & Fixtures
    let identity = AgeIdentity::generate();
    let storage = SqliteStorage::new(db_path, AgeCrypto).await?;
    let registry = VaultRegistry::new(storage);
    let vault = test_vault_with(
        Uuid::new_v4(),
        "Personal",
        true,
        vec![identity.to_recipient()],
    );
    registry.add_vault(&vault).await?;
    let svc = registry.vault_service_for(&vault, identity.clone());

    // -- Exec
    for entry in entries {
        let entry = Entry {
            vault_id: vault.id,
            ..entry.clone()
        };
        svc.save_entry(&entry).await?;
    }

    // -- Check
    Ok((registry, identity, vault))
}
