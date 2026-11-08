//! Entry CRUD and search tests for the SQLite storage adapter.

use super::support::{
    create_test_keys, create_test_storage, make_entry, now, plain_field, secret_field, seed_vault,
};
use crate::domain::{Entry, EntryType, FieldValue};
use crate::ports::EntryStore;
use uuid::Uuid;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::test]
async fn test_save_and_get_plaintext_entry() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await?;
    let entry = make_entry(
        vault_id,
        "Test Entry",
        vec![plain_field("username", "alice", 0)],
    );

    // -- Exec
    storage.save_entry(&entry, &recipients).await?;

    // -- Check
    let retrieved = storage
        .get_entry(entry.id, &identity)
        .await?
        .ok_or("should have entry")?;
    assert_eq!(retrieved.id, entry.id);
    assert_eq!(retrieved.name, "Test Entry");
    assert_eq!(retrieved.fields.len(), 1);
    assert_eq!(retrieved.fields[0].key, "username");
    assert_eq!(
        retrieved.fields[0].value,
        FieldValue::Text("alice".to_string())
    );
    Ok(())
}

#[tokio::test]
async fn test_save_and_get_encrypted_field() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await?;
    let entry = make_entry(
        vault_id,
        "Secure Entry",
        vec![
            plain_field("username", "alice", 0),
            secret_field("password", "supersecret", 1),
        ],
    );

    // -- Exec
    storage.save_entry(&entry, &recipients).await?;

    // -- Check
    let retrieved = storage
        .get_entry(entry.id, &identity)
        .await?
        .ok_or("should have entry")?;
    assert_eq!(retrieved.fields.len(), 2);
    assert_eq!(
        retrieved.fields[1].value,
        FieldValue::Hidden("supersecret".to_string())
    );
    assert!(retrieved.fields[1].encrypted);
    Ok(())
}

#[tokio::test]
async fn test_get_nonexistent_returns_none() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;
    let (_recipients, identity) = create_test_keys();

    // -- Exec
    let result = storage.get_entry(Uuid::new_v4(), &identity).await?;

    // -- Check
    assert!(result.is_none());
    Ok(())
}

#[tokio::test]
async fn test_list_entries_scoped_to_vault() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    let other_vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await?;
    seed_vault(&storage, other_vault_id).await?;
    for i in 0..3 {
        let entry = make_entry(vault_id, &format!("Entry {}", i), vec![]);
        storage.save_entry(&entry, &recipients).await?;
    }
    let other = make_entry(other_vault_id, "Other", vec![]);
    storage.save_entry(&other, &recipients).await?;

    // -- Exec
    let entries = storage.list_entries(vault_id, None, &identity).await?;

    // -- Check
    assert_eq!(entries.len(), 3);
    assert!(entries.iter().all(|e| e.vault_id == vault_id));
    Ok(())
}

#[tokio::test]
async fn test_delete_entry() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await?;
    let entry = make_entry(vault_id, "To Delete", vec![plain_field("k", "v", 0)]);
    storage.save_entry(&entry, &recipients).await?;

    // -- Exec
    storage.delete_entry(entry.id).await?;

    // -- Check
    let result = storage.get_entry(entry.id, &identity).await?;
    assert!(result.is_none());
    Ok(())
}

#[tokio::test]
async fn test_delete_cascades_to_fields() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await?;
    let entry = make_entry(
        vault_id,
        "With Fields",
        vec![
            plain_field("field1", "val1", 0),
            plain_field("field2", "val2", 1),
        ],
    );
    storage.save_entry(&entry, &recipients).await?;

    // -- Exec
    storage.delete_entry(entry.id).await?;

    // -- Check
    let result = storage.get_entry(entry.id, &identity).await?;
    assert!(result.is_none());
    Ok(())
}

#[tokio::test]
async fn test_save_replaces_fields_on_update() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    let entry_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await?;
    let original = Entry {
        id: entry_id,
        vault_id,
        name: "Original".to_string(),
        entry_type: EntryType::Token,
        created_at: now(),
        modified_at: now(),
        fields: vec![plain_field("old_field", "old_value", 0)],
    };
    storage.save_entry(&original, &recipients).await?;
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

    // -- Exec
    storage.save_entry(&updated, &recipients).await?;

    // -- Check
    let retrieved = storage
        .get_entry(entry_id, &identity)
        .await?
        .ok_or("should have entry")?;
    assert_eq!(retrieved.name, "Updated");
    assert_eq!(retrieved.fields.len(), 2);
    assert!(retrieved.fields.iter().all(|f| f.key != "old_field"));
    Ok(())
}

#[tokio::test]
async fn test_search_finds_plaintext_fields() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await?;
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
    storage.save_entry(&entry1, &recipients).await?;
    storage.save_entry(&entry2, &recipients).await?;

    // -- Exec
    let results = storage
        .list_entries(vault_id, Some("octocat"), &identity)
        .await?;

    // -- Check
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "GitHub");
    Ok(())
}

#[tokio::test]
async fn save_entry_updates_vault_id_on_conflict() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;
    let (recipients, identity) = create_test_keys();
    let vault_a = Uuid::new_v4();
    let vault_b = Uuid::new_v4();
    seed_vault(&storage, vault_a).await?;
    seed_vault(&storage, vault_b).await?;
    let entry = make_entry(
        vault_a,
        "Movable Entry",
        vec![plain_field("username", "alice", 0)],
    );
    storage.save_entry(&entry, &recipients).await?;

    // -- Exec
    let entries_a = storage.list_entries(vault_a, None, &identity).await?;
    let moved = Entry {
        vault_id: vault_b,
        ..entry
    };
    storage.save_entry(&moved, &recipients).await?;

    // -- Check
    assert_eq!(entries_a.len(), 1);
    assert_eq!(entries_a[0].id, moved.id);
    let entries_a = storage.list_entries(vault_a, None, &identity).await?;
    assert_eq!(entries_a.len(), 0, "entry should be removed from vault A");
    let entries_b = storage.list_entries(vault_b, None, &identity).await?;
    assert_eq!(entries_b.len(), 1, "entry should appear in vault B");
    assert_eq!(entries_b[0].id, moved.id);
    assert_eq!(entries_b[0].vault_id, vault_b);
    Ok(())
}

#[tokio::test]
async fn test_search_does_not_match_encrypted_fields() -> Result<()> {
    // -- Setup & Fixtures
    let storage = create_test_storage().await?;
    let (recipients, identity) = create_test_keys();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await?;
    let entry = make_entry(
        vault_id,
        "Secret",
        vec![
            plain_field("username", "visible", 0),
            secret_field("password", "invisible", 1),
        ],
    );
    storage.save_entry(&entry, &recipients).await?;

    // -- Exec
    let results = storage
        .list_entries(vault_id, Some("invisible"), &identity)
        .await?;

    // -- Check
    assert_eq!(results.len(), 0, "encrypted fields must not be searchable");
    Ok(())
}
