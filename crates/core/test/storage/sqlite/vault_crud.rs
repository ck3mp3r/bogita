//! Vault CRUD tests for the SQLite storage adapter.

use super::support::{create_test_storage, make_vault};
use crate::domain::Vault;
use crate::ports::VaultStore;
use uuid::Uuid;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::test]
async fn test_save_and_get_vault() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;
    let vault = make_vault("personal", true);

    // -- Exec
    storage.save_vault(&vault).await?;

    // -- Check
    let retrieved = storage
        .get_vault(vault.id)
        .await?
        .ok_or("should have vault")?;
    assert_eq!(retrieved.id, vault.id);
    assert_eq!(retrieved.name, "personal");
    assert!(retrieved.is_default);
    assert_eq!(retrieved.lock_timeout, Some(300));
    assert!(!retrieved.auto_sync);
    assert_eq!(retrieved.recipients.len(), 1);
    Ok(())
}

#[tokio::test]
async fn test_get_nonexistent_vault_returns_none() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;

    // -- Exec
    let result = storage.get_vault(Uuid::new_v4()).await?;

    // -- Check
    assert!(result.is_none());
    Ok(())
}

#[tokio::test]
async fn test_list_vaults() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;
    let v1 = make_vault("vault-a", true);
    let v2 = make_vault("vault-b", false);
    let v3 = make_vault("vault-c", false);
    storage.save_vault(&v1).await?;
    storage.save_vault(&v2).await?;
    storage.save_vault(&v3).await?;

    // -- Exec
    let vaults = storage.list_vaults().await?;

    // -- Check
    assert_eq!(vaults.len(), 3);
    let names: Vec<&str> = vaults.iter().map(|v| v.name.as_str()).collect();
    assert!(names.contains(&"vault-a"));
    assert!(names.contains(&"vault-b"));
    assert!(names.contains(&"vault-c"));
    Ok(())
}

#[tokio::test]
async fn test_delete_vault() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;
    let vault = make_vault("to-delete", false);
    storage.save_vault(&vault).await?;

    // -- Exec
    storage.delete_vault(vault.id).await?;

    // -- Check
    let result = storage.get_vault(vault.id).await?;
    assert!(result.is_none());
    Ok(())
}

#[tokio::test]
async fn test_save_vault_updates_existing() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;
    let vault_id = Uuid::new_v4();
    let original = Vault {
        id: vault_id,
        name: "original-name".to_string(),
        is_default: false,
        created_at: chrono::Utc::now().timestamp(),
        sync_target: None,
        recipients: vec![],
        lock_timeout: None,
        auto_sync: false,
    };
    storage.save_vault(&original).await?;
    let updated = Vault {
        id: vault_id,
        name: "updated-name".to_string(),
        is_default: true,
        lock_timeout: Some(600),
        auto_sync: true,
        ..original
    };

    // -- Exec
    storage.save_vault(&updated).await?;

    // -- Check
    let retrieved = storage
        .get_vault(vault_id)
        .await?
        .ok_or("should have vault")?;
    assert_eq!(retrieved.name, "updated-name");
    assert!(retrieved.is_default);
    assert_eq!(retrieved.lock_timeout, Some(600));
    assert!(retrieved.auto_sync);
    Ok(())
}
