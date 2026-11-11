//! Tests for VaultEntryService
//!
//! TDD: RED → GREEN → REFACTOR
//! Uses real SqliteStorage (:memory:) and real AgeCrypto — no mocks.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::crypto::age::AgeCrypto;
use crate::domain::{AgeIdentity, Entry, EntryType, Field, FieldType, FieldValue};
use crate::storage::sqlite::SqliteStorage;
use crate::test_support::{test_entry, test_field};
use crate::vault::service::VaultEntryService;
use uuid::Uuid;

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn create_test_keys() -> (Vec<crate::domain::AgeRecipient>, AgeIdentity) {
    let identity = AgeIdentity::generate();
    let recipient = identity.to_recipient();
    (vec![recipient], identity)
}

async fn create_service() -> Result<(VaultEntryService<SqliteStorage<AgeCrypto>>, Uuid)> {
    let storage = SqliteStorage::new(":memory:", AgeCrypto).await?;
    let vault_id = Uuid::new_v4();
    crate::test_support::seed_vault(&storage, vault_id).await?;
    let (recipients, identity) = create_test_keys();
    Ok((
        VaultEntryService::new(storage, recipients, identity),
        vault_id,
    ))
}

fn make_entry(vault_id: Uuid, name: &str) -> Entry {
    test_entry(
        vault_id,
        name,
        vec![
            test_field(
                "username",
                FieldValue::Text("alice".to_string()),
                FieldType::Username,
                false,
                0,
            ),
            test_field(
                "password",
                FieldValue::Hidden("secret".to_string()),
                FieldType::Token,
                true,
                1,
            ),
        ],
    )
}

#[tokio::test]
async fn test_add_and_get_entry() -> Result<()> {
    let (service, vault_id) = create_service().await?;

    let entry = make_entry(vault_id, "GitHub");
    service.save_entry(&entry).await?;

    let retrieved = service
        .get_entry(entry.id)
        .await?
        .ok_or("should have entry")?;

    assert_eq!(retrieved.id, entry.id);
    assert_eq!(retrieved.name, "GitHub");
    assert_eq!(retrieved.fields.len(), 2);
    assert_eq!(
        retrieved.fields[0].value,
        FieldValue::Text("alice".to_string())
    );
    assert_eq!(
        retrieved.fields[1].value,
        FieldValue::Hidden("secret".to_string())
    );
    Ok(())
}

#[tokio::test]
async fn test_get_nonexistent_returns_none() -> Result<()> {
    let (service, _vault_id) = create_service().await?;

    let result = service.get_entry(Uuid::new_v4()).await?;

    assert!(result.is_none());
    Ok(())
}

#[tokio::test]
async fn test_list_all_entries() -> Result<()> {
    let (service, vault_id) = create_service().await?;

    service.save_entry(&make_entry(vault_id, "GitHub")).await?;
    service.save_entry(&make_entry(vault_id, "GitLab")).await?;

    let entries = service.list_entries(vault_id, None).await?;

    assert_eq!(entries.len(), 2);
    Ok(())
}

#[tokio::test]
async fn test_list_entries_with_search() -> Result<()> {
    let (service, vault_id) = create_service().await?;

    service.save_entry(&make_entry(vault_id, "GitHub")).await?;
    service.save_entry(&make_entry(vault_id, "GitLab")).await?;
    service.save_entry(&make_entry(vault_id, "AWS")).await?;

    let results = service.list_entries(vault_id, Some("Git")).await?;

    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|e| e.name == "GitHub"));
    assert!(results.iter().any(|e| e.name == "GitLab"));
    Ok(())
}

#[tokio::test]
async fn test_update_entry() -> Result<()> {
    let (service, vault_id) = create_service().await?;

    let entry = make_entry(vault_id, "GitHub");
    service.save_entry(&entry).await?;

    let updated = Entry {
        id: entry.id,
        vault_id,
        name: "GitHub Updated".to_string(),
        entry_type: EntryType::Token,
        created_at: entry.created_at,
        modified_at: now(),
        fields: vec![Field {
            id: Uuid::new_v4(),
            key: "username".to_string(),
            value: FieldValue::Text("bob".to_string()),
            field_type: FieldType::Username,
            encrypted: false,
            idx: 0,
        }],
    };
    service.save_entry(&updated).await?;

    let retrieved = service
        .get_entry(entry.id)
        .await?
        .ok_or("should have entry")?;

    assert_eq!(retrieved.name, "GitHub Updated");
    assert_eq!(retrieved.fields.len(), 1);
    assert_eq!(
        retrieved.fields[0].value,
        FieldValue::Text("bob".to_string())
    );
    Ok(())
}

#[tokio::test]
async fn test_delete_entry() -> Result<()> {
    let (service, vault_id) = create_service().await?;

    let entry = make_entry(vault_id, "To Delete");
    service.save_entry(&entry).await?;
    service.delete_entry(entry.id).await?;

    let result = service.get_entry(entry.id).await?;

    assert!(result.is_none());
    Ok(())
}
