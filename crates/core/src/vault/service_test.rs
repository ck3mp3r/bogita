//! Tests for VaultService
//!
//! TDD: RED → GREEN → REFACTOR
//! Uses real SqliteStorage (:memory:) and real AgeCrypto — no mocks.

use crate::crypto::age::AgeCrypto;
use crate::domain::{AgeIdentity, Entry, EntryType, Field, FieldType, FieldValue};
use crate::storage::sqlite::SqliteStorage;
use crate::vault::service::VaultService;
use uuid::Uuid;

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn create_test_keys() -> (Vec<crate::domain::AgeRecipient>, AgeIdentity) {
    let identity = AgeIdentity::generate();
    let recipient = identity.to_recipient();
    (vec![recipient], identity)
}

async fn create_service() -> VaultService<SqliteStorage<AgeCrypto>, AgeCrypto> {
    let storage = SqliteStorage::new(":memory:", AgeCrypto)
        .await
        .expect("storage init failed");
    let (recipients, identity) = create_test_keys();
    VaultService::new(storage, AgeCrypto, recipients, identity)
}

fn make_entry(vault_id: Uuid, name: &str) -> Entry {
    Entry {
        id: Uuid::new_v4(),
        vault_id,
        name: name.to_string(),
        entry_type: EntryType::Token,
        created_at: now(),
        modified_at: now(),
        fields: vec![
            Field {
                id: Uuid::new_v4(),
                key: "username".to_string(),
                value: FieldValue::Text("alice".to_string()),
                field_type: FieldType::Username,
                encrypted: false,
                idx: 0,
            },
            Field {
                id: Uuid::new_v4(),
                key: "password".to_string(),
                value: FieldValue::Hidden("secret".to_string()),
                field_type: FieldType::Token,
                encrypted: true,
                idx: 1,
            },
        ],
    }
}

#[tokio::test]
async fn test_add_and_get_entry() {
    let service = create_service().await;
    let vault_id = Uuid::new_v4();
    service
        .seed_vault_for_test(vault_id)
        .await
        .expect("seed failed");

    let entry = make_entry(vault_id, "GitHub");
    service.add_entry(&entry).await.expect("add_entry failed");

    let retrieved = service
        .get_entry(entry.id)
        .await
        .expect("get_entry failed")
        .expect("entry should exist");

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
}

#[tokio::test]
async fn test_get_nonexistent_returns_none() {
    let service = create_service().await;

    let result = service
        .get_entry(Uuid::new_v4())
        .await
        .expect("get_entry should not error");

    assert!(result.is_none());
}

#[tokio::test]
async fn test_list_all_entries() {
    let service = create_service().await;
    let vault_id = Uuid::new_v4();
    service
        .seed_vault_for_test(vault_id)
        .await
        .expect("seed failed");

    service
        .add_entry(&make_entry(vault_id, "GitHub"))
        .await
        .expect("add failed");
    service
        .add_entry(&make_entry(vault_id, "GitLab"))
        .await
        .expect("add failed");

    let entries = service
        .list_entries(vault_id, None)
        .await
        .expect("list_entries failed");

    assert_eq!(entries.len(), 2);
}

#[tokio::test]
async fn test_list_entries_with_search() {
    let service = create_service().await;
    let vault_id = Uuid::new_v4();
    service
        .seed_vault_for_test(vault_id)
        .await
        .expect("seed failed");

    service
        .add_entry(&make_entry(vault_id, "GitHub"))
        .await
        .expect("add failed");
    service
        .add_entry(&make_entry(vault_id, "GitLab"))
        .await
        .expect("add failed");
    service
        .add_entry(&make_entry(vault_id, "AWS"))
        .await
        .expect("add failed");

    let results = service
        .list_entries(vault_id, Some("Git"))
        .await
        .expect("list_entries failed");

    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|e| e.name == "GitHub"));
    assert!(results.iter().any(|e| e.name == "GitLab"));
}

#[tokio::test]
async fn test_update_entry() {
    let service = create_service().await;
    let vault_id = Uuid::new_v4();
    service
        .seed_vault_for_test(vault_id)
        .await
        .expect("seed failed");

    let entry = make_entry(vault_id, "GitHub");
    service.add_entry(&entry).await.expect("add failed");

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
    service.update_entry(&updated).await.expect("update failed");

    let retrieved = service
        .get_entry(entry.id)
        .await
        .expect("get failed")
        .expect("entry should exist");

    assert_eq!(retrieved.name, "GitHub Updated");
    assert_eq!(retrieved.fields.len(), 1);
    assert_eq!(
        retrieved.fields[0].value,
        FieldValue::Text("bob".to_string())
    );
}

#[tokio::test]
async fn test_delete_entry() {
    let service = create_service().await;
    let vault_id = Uuid::new_v4();
    service
        .seed_vault_for_test(vault_id)
        .await
        .expect("seed failed");

    let entry = make_entry(vault_id, "To Delete");
    service.add_entry(&entry).await.expect("add failed");
    service.delete_entry(entry.id).await.expect("delete failed");

    let result = service
        .get_entry(entry.id)
        .await
        .expect("get should not error");

    assert!(result.is_none());
}
