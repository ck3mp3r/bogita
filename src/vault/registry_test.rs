//! Tests for VaultRegistry
//!
//! TDD: RED → GREEN → REFACTOR
//! Uses real SqliteStorage (:memory:) and real AgeCrypto — no mocks.

use crate::crypto::age::AgeCrypto;
use crate::domain::{AgeIdentity, SqliteConfig, Vault, VaultBackend};
use crate::storage::sqlite::SqliteStorage;
use crate::vault::registry::VaultRegistry;
use uuid::Uuid;

fn make_vault(name: &str, is_default: bool) -> Vault {
    let identity = AgeIdentity::generate();
    let recipient = identity.to_recipient();
    Vault {
        id: Uuid::new_v4(),
        name: name.to_string(),
        is_default,
        created_at: chrono::Utc::now().timestamp(),
        backend: VaultBackend::Sqlite(SqliteConfig {
            path: ":memory:".to_string(),
        }),
        recipients: vec![recipient],
        lock_timeout: Some(300),
        auto_sync: false,
    }
}

async fn create_registry() -> VaultRegistry<SqliteStorage<AgeCrypto>, AgeCrypto> {
    let storage = SqliteStorage::new(":memory:", AgeCrypto)
        .await
        .expect("storage init failed");
    VaultRegistry::new(storage, AgeCrypto)
}

#[tokio::test]
async fn test_add_and_list_vault() {
    let registry = create_registry().await;
    let vault = make_vault("personal", true);

    registry.add_vault(&vault).await.expect("add_vault failed");

    let vaults = registry.list_vaults().await.expect("list_vaults failed");
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].name, "personal");
    assert!(vaults[0].is_default);
}

#[tokio::test]
async fn test_add_multiple_vaults() {
    let registry = create_registry().await;

    registry
        .add_vault(&make_vault("work", false))
        .await
        .expect("add failed");
    registry
        .add_vault(&make_vault("personal", true))
        .await
        .expect("add failed");
    registry
        .add_vault(&make_vault("dev", false))
        .await
        .expect("add failed");

    let vaults = registry.list_vaults().await.expect("list failed");
    assert_eq!(vaults.len(), 3);
}

#[tokio::test]
async fn test_remove_vault() {
    let registry = create_registry().await;
    let vault = make_vault("to-remove", false);
    registry.add_vault(&vault).await.expect("add failed");

    registry
        .remove_vault(vault.id)
        .await
        .expect("remove_vault failed");

    let vaults = registry.list_vaults().await.expect("list failed");
    assert!(vaults.is_empty());
}

#[tokio::test]
async fn test_set_default_clears_previous_default() {
    let registry = create_registry().await;

    let v1 = make_vault("vault-a", true);
    let v2 = make_vault("vault-b", false);
    registry.add_vault(&v1).await.expect("add failed");
    registry.add_vault(&v2).await.expect("add failed");

    registry
        .set_default(v2.id)
        .await
        .expect("set_default failed");

    let vaults = registry.list_vaults().await.expect("list failed");
    let defaults: Vec<_> = vaults.iter().filter(|v| v.is_default).collect();
    assert_eq!(defaults.len(), 1, "exactly one vault should be default");
    assert_eq!(defaults[0].id, v2.id);
}

#[tokio::test]
async fn test_default_vault_returns_is_default_vault() {
    let registry = create_registry().await;

    registry
        .add_vault(&make_vault("work", false))
        .await
        .expect("add failed");
    let personal = make_vault("personal", true);
    registry.add_vault(&personal).await.expect("add failed");

    let default = registry
        .default_vault()
        .await
        .expect("default_vault failed")
        .expect("should have a default vault");

    assert_eq!(default.id, personal.id);
    assert!(default.is_default);
}

#[tokio::test]
async fn test_default_vault_returns_none_when_none_set() {
    let registry = create_registry().await;

    registry
        .add_vault(&make_vault("work", false))
        .await
        .expect("add failed");

    let default = registry
        .default_vault()
        .await
        .expect("default_vault failed");
    assert!(default.is_none());
}

#[tokio::test]
async fn test_vault_service_for_can_add_entry() {
    use crate::domain::{Entry, EntryType, Field, FieldType, FieldValue};

    let registry = create_registry().await;
    let vault = make_vault("personal", true);
    registry.add_vault(&vault).await.expect("add failed");

    let identity = AgeIdentity::generate();
    let service = registry.vault_service_for(&vault, identity.clone());

    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: vault.id,
        name: "GitHub".to_string(),
        entry_type: EntryType::Password,
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

    service.add_entry(&entry).await.expect("add_entry failed");

    let retrieved = service
        .get_entry(entry.id)
        .await
        .expect("get_entry failed")
        .expect("entry should exist");

    assert_eq!(retrieved.name, "GitHub");
}
