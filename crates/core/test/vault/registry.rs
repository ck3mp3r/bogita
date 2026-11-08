//! Tests for VaultRegistry
//!
//! TDD: RED → GREEN → REFACTOR
//! Uses real SqliteStorage (:memory:) and real AgeCrypto — no mocks.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::crypto::age::AgeCrypto;
use crate::domain::AgeIdentity;
use crate::storage::sqlite::SqliteStorage;
use crate::test_support::test_vault_locked as make_vault;
use crate::vault::registry::VaultRegistry;
use uuid::Uuid;

async fn create_registry() -> Result<VaultRegistry<SqliteStorage<AgeCrypto>>> {
    let storage = SqliteStorage::new(":memory:", AgeCrypto).await?;
    Ok(VaultRegistry::new(storage))
}

#[tokio::test]
async fn test_add_and_list_vault() -> Result<()> {
    let registry = create_registry().await?;
    let vault = make_vault("personal", true);

    registry.add_vault(&vault).await?;

    let vaults = registry.list_vaults().await?;
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].name, "personal");
    assert!(vaults[0].is_default);
    Ok(())
}

#[tokio::test]
async fn test_add_multiple_vaults() -> Result<()> {
    let registry = create_registry().await?;

    registry.add_vault(&make_vault("work", false)).await?;
    registry.add_vault(&make_vault("personal", true)).await?;
    registry.add_vault(&make_vault("dev", false)).await?;

    let vaults = registry.list_vaults().await?;
    assert_eq!(vaults.len(), 3);
    Ok(())
}

#[tokio::test]
async fn test_remove_vault() -> Result<()> {
    let registry = create_registry().await?;
    let vault = make_vault("to-remove", false);
    registry.add_vault(&vault).await?;

    registry.remove_vault(vault.id).await?;

    let vaults = registry.list_vaults().await?;
    assert!(vaults.is_empty());
    Ok(())
}

#[tokio::test]
async fn test_set_default_clears_previous_default() -> Result<()> {
    let registry = create_registry().await?;

    let v1 = make_vault("vault-a", true);
    let v2 = make_vault("vault-b", false);
    registry.add_vault(&v1).await?;
    registry.add_vault(&v2).await?;

    registry.set_default(v2.id).await?;

    let vaults = registry.list_vaults().await?;
    let defaults: Vec<_> = vaults.iter().filter(|v| v.is_default).collect();
    assert_eq!(defaults.len(), 1, "exactly one vault should be default");
    assert_eq!(defaults[0].id, v2.id);
    Ok(())
}

#[tokio::test]
async fn test_default_vault_returns_is_default_vault() -> Result<()> {
    let registry = create_registry().await?;

    registry.add_vault(&make_vault("work", false)).await?;
    let personal = make_vault("personal", true);
    registry.add_vault(&personal).await?;

    let default = registry
        .default_vault()
        .await?
        .ok_or("should have a default vault")?;

    assert_eq!(default.id, personal.id);
    assert!(default.is_default);
    Ok(())
}

#[tokio::test]
async fn test_default_vault_returns_none_when_none_set() -> Result<()> {
    let registry = create_registry().await?;

    registry.add_vault(&make_vault("work", false)).await?;

    let default = registry.default_vault().await?;
    assert!(default.is_none());
    Ok(())
}

#[tokio::test]
async fn test_vault_service_for_can_add_entry() -> Result<()> {
    use crate::domain::{Entry, EntryType, Field, FieldType, FieldValue};

    let registry = create_registry().await?;
    let vault = make_vault("personal", true);
    registry.add_vault(&vault).await?;

    let identity = AgeIdentity::generate();
    let service = registry.vault_service_for(&vault, identity.clone());

    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: vault.id,
        name: "GitHub".to_string(),
        entry_type: EntryType::Token,
        created_at: chrono::Utc::now().timestamp(),
        modified_at: chrono::Utc::now().timestamp(),
        fields: vec![Field {
            id: Uuid::new_v4(),
            key: "username".to_string(),
            value: FieldValue::Text("alice".to_string()),
            field_type: FieldType::Username,
            encrypted: false,
            idx: 0,
        }],
    };

    service.save_entry(&entry).await?;

    let retrieved = service
        .get_entry(entry.id)
        .await?
        .ok_or("should have entry")?;

    assert_eq!(retrieved.name, "GitHub");
    Ok(())
}
