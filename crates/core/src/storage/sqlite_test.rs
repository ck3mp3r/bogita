//! Tests for SQLite storage adapter
//!
//! TDD: RED → GREEN → REFACTOR

use crate::crypto::age::AgeCrypto;
use crate::domain::{
    AgeIdentity, AgeRecipient, Entry, EntryType, Field, FieldType, FieldValue, Vault,
};
use crate::ports::{EntryStore, VaultStore};
use crate::storage::sqlite::SqliteStorage;
use uuid::Uuid;

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

async fn create_test_storage() -> SqliteStorage<AgeCrypto> {
    SqliteStorage::new(":memory:", AgeCrypto)
        .await
        .expect("Failed to create in-memory storage")
}

fn create_test_keys() -> (Vec<AgeRecipient>, AgeIdentity) {
    let identity = AgeIdentity::generate();
    let recipient = identity.to_recipient();
    (vec![recipient], identity)
}

/// Insert a minimal vault row so FK constraints pass during tests
async fn seed_vault(storage: &SqliteStorage<AgeCrypto>, vault_id: Uuid) {
    storage
        .seed_vault_for_test(vault_id)
        .await
        .expect("Failed to seed vault");
}

fn make_entry(vault_id: Uuid, name: &str, fields: Vec<Field>) -> Entry {
    Entry {
        id: Uuid::new_v4(),
        vault_id,
        name: name.to_string(),
        entry_type: EntryType::Token,
        created_at: now(),
        modified_at: now(),
        fields,
    }
}

fn plain_field(key: &str, value: &str, idx: i32) -> Field {
    Field {
        id: Uuid::new_v4(),
        key: key.to_string(),
        value: FieldValue::Text(value.to_string()),
        field_type: FieldType::Username,
        encrypted: false,
        idx,
    }
}

fn secret_field(key: &str, value: &str, idx: i32) -> Field {
    Field {
        id: Uuid::new_v4(),
        key: key.to_string(),
        value: FieldValue::Hidden(value.to_string()),
        field_type: FieldType::Token,
        encrypted: true,
        idx,
    }
}

#[tokio::test]
async fn test_save_and_get_plaintext_entry() {
    let storage = create_test_storage().await;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await;

    let entry = make_entry(
        vault_id,
        "Test Entry",
        vec![plain_field("username", "alice", 0)],
    );

    storage
        .save_entry(&entry, &recipients)
        .await
        .expect("save should succeed");

    let retrieved = storage
        .get_entry(entry.id, &identity)
        .await
        .expect("get should succeed")
        .expect("entry should exist");

    assert_eq!(retrieved.id, entry.id);
    assert_eq!(retrieved.name, "Test Entry");
    assert_eq!(retrieved.fields.len(), 1);
    assert_eq!(retrieved.fields[0].key, "username");
    assert_eq!(
        retrieved.fields[0].value,
        FieldValue::Text("alice".to_string())
    );
}

#[tokio::test]
async fn test_save_and_get_encrypted_field() {
    let storage = create_test_storage().await;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await;

    let entry = make_entry(
        vault_id,
        "Secure Entry",
        vec![
            plain_field("username", "alice", 0),
            secret_field("password", "supersecret", 1),
        ],
    );

    storage
        .save_entry(&entry, &recipients)
        .await
        .expect("save should succeed");

    let retrieved = storage
        .get_entry(entry.id, &identity)
        .await
        .expect("get should succeed")
        .expect("entry should exist");

    assert_eq!(retrieved.fields.len(), 2);
    assert_eq!(
        retrieved.fields[1].value,
        FieldValue::Hidden("supersecret".to_string())
    );
    assert!(retrieved.fields[1].encrypted);
}

#[tokio::test]
async fn test_get_nonexistent_returns_none() {
    let storage = create_test_storage().await;
    let (_recipients, identity) = create_test_keys();

    let result = storage
        .get_entry(Uuid::new_v4(), &identity)
        .await
        .expect("get should not error");

    assert!(result.is_none());
}

#[tokio::test]
async fn test_list_entries_scoped_to_vault() {
    let storage = create_test_storage().await;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    let other_vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await;
    seed_vault(&storage, other_vault_id).await;

    for i in 0..3 {
        let entry = make_entry(vault_id, &format!("Entry {}", i), vec![]);
        storage
            .save_entry(&entry, &recipients)
            .await
            .expect("save failed");
    }

    let other = make_entry(other_vault_id, "Other", vec![]);
    storage
        .save_entry(&other, &recipients)
        .await
        .expect("save failed");

    let entries = storage
        .list_entries(vault_id, None, &identity)
        .await
        .expect("list should succeed");

    assert_eq!(entries.len(), 3);
    assert!(entries.iter().all(|e| e.vault_id == vault_id));
}

#[tokio::test]
async fn test_delete_entry() {
    let storage = create_test_storage().await;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await;

    let entry = make_entry(vault_id, "To Delete", vec![plain_field("k", "v", 0)]);
    storage
        .save_entry(&entry, &recipients)
        .await
        .expect("save failed");

    storage
        .delete_entry(entry.id)
        .await
        .expect("delete should succeed");

    let result = storage
        .get_entry(entry.id, &identity)
        .await
        .expect("get should not error");

    assert!(result.is_none());
}

#[tokio::test]
async fn test_delete_cascades_to_fields() {
    let storage = create_test_storage().await;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await;

    let entry = make_entry(
        vault_id,
        "With Fields",
        vec![
            plain_field("field1", "val1", 0),
            plain_field("field2", "val2", 1),
        ],
    );
    storage
        .save_entry(&entry, &recipients)
        .await
        .expect("save failed");
    storage.delete_entry(entry.id).await.expect("delete failed");

    let result = storage
        .get_entry(entry.id, &identity)
        .await
        .expect("get should not error");

    assert!(result.is_none());
}

#[tokio::test]
async fn test_save_replaces_fields_on_update() {
    let storage = create_test_storage().await;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    let entry_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await;

    let original = Entry {
        id: entry_id,
        vault_id,
        name: "Original".to_string(),
        entry_type: EntryType::Token,
        created_at: now(),
        modified_at: now(),
        fields: vec![plain_field("old_field", "old_value", 0)],
    };
    storage
        .save_entry(&original, &recipients)
        .await
        .expect("save failed");

    let updated = Entry {
        id: entry_id,
        vault_id,
        name: "Updated".to_string(),
        entry_type: EntryType::Token,
        created_at: original.created_at,
        modified_at: now(),
        fields: vec![
            plain_field("new_field1", "new_value1", 0),
            plain_field("new_field2", "new_value2", 1),
        ],
    };
    storage
        .save_entry(&updated, &recipients)
        .await
        .expect("update failed");

    let retrieved = storage
        .get_entry(entry_id, &identity)
        .await
        .expect("get failed")
        .expect("entry should exist");

    assert_eq!(retrieved.name, "Updated");
    assert_eq!(retrieved.fields.len(), 2);
    assert!(retrieved.fields.iter().all(|f| f.key != "old_field"));
}

#[tokio::test]
async fn test_search_finds_plaintext_fields() {
    let storage = create_test_storage().await;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await;

    let entry1 = make_entry(
        vault_id,
        "GitHub",
        vec![plain_field("username", "octocat", 0)],
    );
    let entry2 = make_entry(
        vault_id,
        "GitLab",
        vec![plain_field("username", "tanuki", 0)],
    );

    storage
        .save_entry(&entry1, &recipients)
        .await
        .expect("save failed");
    storage
        .save_entry(&entry2, &recipients)
        .await
        .expect("save failed");

    let results = storage
        .list_entries(vault_id, Some("octocat"), &identity)
        .await
        .expect("search failed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "GitHub");
}

#[tokio::test]
async fn save_entry_updates_vault_id_on_conflict() {
    let storage = create_test_storage().await;
    let (recipients, identity) = create_test_keys();
    let vault_a = Uuid::new_v4();
    let vault_b = Uuid::new_v4();
    seed_vault(&storage, vault_a).await;
    seed_vault(&storage, vault_b).await;

    let entry = make_entry(
        vault_a,
        "Movable Entry",
        vec![plain_field("username", "alice", 0)],
    );

    // Save to vault A
    storage
        .save_entry(&entry, &recipients)
        .await
        .expect("initial save should succeed");

    // Verify it appears in vault A
    let entries_a = storage
        .list_entries(vault_a, None, &identity)
        .await
        .expect("list vault A should succeed");
    assert_eq!(entries_a.len(), 1);
    assert_eq!(entries_a[0].id, entry.id);

    // Update entry to vault B
    let moved = Entry {
        vault_id: vault_b,
        ..entry
    };
    storage
        .save_entry(&moved, &recipients)
        .await
        .expect("update vault_id should succeed");

    // Verify it no longer appears in vault A
    let entries_a = storage
        .list_entries(vault_a, None, &identity)
        .await
        .expect("list vault A should succeed");
    assert_eq!(entries_a.len(), 0, "entry should be removed from vault A");

    // Verify it now appears in vault B
    let entries_b = storage
        .list_entries(vault_b, None, &identity)
        .await
        .expect("list vault B should succeed");
    assert_eq!(entries_b.len(), 1, "entry should appear in vault B");
    assert_eq!(entries_b[0].id, entry.id);
    assert_eq!(entries_b[0].vault_id, vault_b);
}

#[tokio::test]
async fn test_search_does_not_match_encrypted_fields() {
    let storage = create_test_storage().await;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await;

    let entry = make_entry(
        vault_id,
        "Secret",
        vec![
            plain_field("username", "visible", 0),
            secret_field("password", "invisible", 1),
        ],
    );
    storage
        .save_entry(&entry, &recipients)
        .await
        .expect("save failed");

    let results = storage
        .list_entries(vault_id, Some("invisible"), &identity)
        .await
        .expect("search failed");

    assert_eq!(results.len(), 0, "encrypted fields must not be searchable");
}

// ============================================================================
// Vault CRUD tests
// ============================================================================

fn make_vault(name: &str, is_default: bool) -> Vault {
    let identity = AgeIdentity::generate();
    let recipient = identity.to_recipient();
    Vault {
        id: Uuid::new_v4(),
        name: name.to_string(),
        is_default,
        created_at: chrono::Utc::now().timestamp(),
        sync_target: None,
        recipients: vec![recipient],
        lock_timeout: Some(300),
        auto_sync: false,
    }
}

#[tokio::test]
async fn test_save_and_get_vault() {
    let storage = create_test_storage().await;
    let vault = make_vault("personal", true);

    storage
        .save_vault(&vault)
        .await
        .expect("save_vault should succeed");

    let retrieved = storage
        .get_vault(vault.id)
        .await
        .expect("get_vault should not error")
        .expect("vault should exist");

    assert_eq!(retrieved.id, vault.id);
    assert_eq!(retrieved.name, "personal");
    assert!(retrieved.is_default);
    assert_eq!(retrieved.lock_timeout, Some(300));
    assert!(!retrieved.auto_sync);
    assert_eq!(retrieved.recipients.len(), 1);
}

#[tokio::test]
async fn test_get_nonexistent_vault_returns_none() {
    let storage = create_test_storage().await;

    let result = storage
        .get_vault(Uuid::new_v4())
        .await
        .expect("get_vault should not error");

    assert!(result.is_none());
}

#[tokio::test]
async fn test_list_vaults() {
    let storage = create_test_storage().await;
    let v1 = make_vault("vault-a", true);
    let v2 = make_vault("vault-b", false);
    let v3 = make_vault("vault-c", false);

    storage.save_vault(&v1).await.expect("save failed");
    storage.save_vault(&v2).await.expect("save failed");
    storage.save_vault(&v3).await.expect("save failed");

    let vaults = storage
        .list_vaults()
        .await
        .expect("list_vaults should succeed");
    assert_eq!(vaults.len(), 3);

    let names: Vec<&str> = vaults.iter().map(|v| v.name.as_str()).collect();
    assert!(names.contains(&"vault-a"));
    assert!(names.contains(&"vault-b"));
    assert!(names.contains(&"vault-c"));
}

#[tokio::test]
async fn test_delete_vault() {
    let storage = create_test_storage().await;
    let vault = make_vault("to-delete", false);

    storage.save_vault(&vault).await.expect("save failed");
    storage
        .delete_vault(vault.id)
        .await
        .expect("delete_vault should succeed");

    let result = storage
        .get_vault(vault.id)
        .await
        .expect("get_vault should not error");

    assert!(result.is_none());
}

#[tokio::test]
async fn test_save_vault_updates_existing() {
    let storage = create_test_storage().await;
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
    storage
        .save_vault(&original)
        .await
        .expect("initial save failed");

    let updated = Vault {
        id: vault_id,
        name: "updated-name".to_string(),
        is_default: true,
        lock_timeout: Some(600),
        auto_sync: true,
        ..original
    };
    storage.save_vault(&updated).await.expect("update failed");

    let retrieved = storage
        .get_vault(vault_id)
        .await
        .expect("get failed")
        .expect("should exist");

    assert_eq!(retrieved.name, "updated-name");
    assert!(retrieved.is_default);
    assert_eq!(retrieved.lock_timeout, Some(600));
    assert!(retrieved.auto_sync);
}
