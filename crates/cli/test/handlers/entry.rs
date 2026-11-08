use crate::args::EntryCommands;
use crate::handlers::entry::{handle_get, handle_ls, handle_search, EntryOutput};
use bogita_core::crypto::AgeCrypto;
use bogita_core::domain::{AgeIdentity, Entry, EntryType, Field, FieldType, FieldValue, Vault};
use bogita_core::storage::sqlite::SqliteStorage;
use bogita_core::vault::registry::VaultRegistry;
use chrono::Utc;
use uuid::Uuid;

fn now() -> i64 {
    Utc::now().timestamp()
}

fn make_password_entry(vault_id: Uuid, name: &str) -> Entry {
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
                value: FieldValue::Hidden("s3cr3t".to_string()),
                field_type: FieldType::Token,
                encrypted: true,
                idx: 1,
            },
        ],
    }
}

fn make_entry(vault_id: Uuid, name: &str, username: &str) -> Entry {
    Entry {
        id: Uuid::new_v4(),
        vault_id,
        name: name.to_string(),
        entry_type: EntryType::Token,
        created_at: now(),
        modified_at: now(),
        fields: vec![Field {
            id: Uuid::new_v4(),
            key: "username".to_string(),
            value: FieldValue::Text(username.to_string()),
            field_type: FieldType::Username,
            encrypted: false,
            idx: 0,
        }],
    }
}

async fn make_registry_single(
    db_path: &std::path::Path,
) -> (
    VaultRegistry<SqliteStorage<AgeCrypto>, AgeCrypto>,
    AgeIdentity,
    Vault,
) {
    let identity = AgeIdentity::generate();
    let storage = SqliteStorage::new(db_path, AgeCrypto).await.unwrap();
    let registry = VaultRegistry::new(storage, AgeCrypto);
    let vault = Vault {
        id: Uuid::new_v4(),
        name: "Personal".to_string(),
        is_default: true,
        created_at: now(),
        sync_target: None,
        recipients: vec![identity.to_recipient()],
        lock_timeout: None,
        auto_sync: false,
    };
    registry.add_vault(&vault).await.unwrap();
    let svc = registry.vault_service_for(&vault, identity.clone());
    svc.add_entry(&make_password_entry(vault.id, "GitHub"))
        .await
        .unwrap();
    (registry, identity, vault)
}

async fn make_registry_multi(
    db_path: &std::path::Path,
) -> (
    VaultRegistry<SqliteStorage<AgeCrypto>, AgeCrypto>,
    AgeIdentity,
    Vault,
) {
    let identity = AgeIdentity::generate();
    let storage = SqliteStorage::new(db_path, AgeCrypto).await.unwrap();
    let registry = VaultRegistry::new(storage, AgeCrypto);
    let vault = Vault {
        id: Uuid::new_v4(),
        name: "Personal".to_string(),
        is_default: true,
        created_at: now(),
        sync_target: None,
        recipients: vec![identity.to_recipient()],
        lock_timeout: None,
        auto_sync: false,
    };
    registry.add_vault(&vault).await.unwrap();
    let svc = registry.vault_service_for(&vault, identity.clone());
    svc.add_entry(&make_entry(vault.id, "GitHub", "alice"))
        .await
        .unwrap();
    svc.add_entry(&make_entry(vault.id, "Gitlab", "bob"))
        .await
        .unwrap();
    svc.add_entry(&make_entry(vault.id, "AWS", "carol"))
        .await
        .unwrap();
    (registry, identity, vault)
}

// ── ls ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn ls_default_vault_returns_entries() {
    let dir = tempfile::tempdir().unwrap();
    let (registry, identity, _vault) = make_registry_single(&dir.path().join("test.db")).await;
    let cmd = EntryCommands::Ls { vault: None };
    let output = handle_ls(cmd, registry, &identity).await.unwrap();
    assert!(matches!(output, EntryOutput::List(ref v) if v.len() == 1 && v[0].name == "GitHub"));
}

#[tokio::test]
async fn ls_named_vault_returns_entries() {
    let dir = tempfile::tempdir().unwrap();
    let (registry, identity, _vault) = make_registry_single(&dir.path().join("test.db")).await;
    let cmd = EntryCommands::Ls {
        vault: Some("Personal".to_string()),
    };
    let output = handle_ls(cmd, registry, &identity).await.unwrap();
    assert!(matches!(output, EntryOutput::List(ref v) if !v.is_empty()));
}

#[tokio::test]
async fn ls_unknown_vault_errors() {
    let dir = tempfile::tempdir().unwrap();
    let (registry, identity, _vault) = make_registry_single(&dir.path().join("test.db")).await;
    let cmd = EntryCommands::Ls {
        vault: Some("nonexistent".to_string()),
    };
    let result = handle_ls(cmd, registry, &identity).await;
    assert!(result.is_err());
}

// ── get ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_returns_entry_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let (registry, identity, _vault) = make_registry_single(&dir.path().join("test.db")).await;
    let cmd = EntryCommands::Get {
        name: "GitHub".to_string(),
        field: None,
        vault: None,
    };
    let output = handle_get(cmd, registry, &identity).await.unwrap();
    if let EntryOutput::Entry(entry) = output {
        assert_eq!(entry.name, "GitHub");
    } else {
        panic!("expected Entry variant");
    }
}

#[tokio::test]
async fn get_unknown_entry_errors() {
    let dir = tempfile::tempdir().unwrap();
    let (registry, identity, _vault) = make_registry_single(&dir.path().join("test.db")).await;
    let cmd = EntryCommands::Get {
        name: "does-not-exist".to_string(),
        field: None,
        vault: None,
    };
    let result = handle_get(cmd, registry, &identity).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn get_field_returns_value() {
    let dir = tempfile::tempdir().unwrap();
    let (registry, identity, _vault) = make_registry_single(&dir.path().join("test.db")).await;
    let cmd = EntryCommands::Get {
        name: "GitHub".to_string(),
        field: Some("username".to_string()),
        vault: None,
    };
    let output = handle_get(cmd, registry, &identity).await.unwrap();
    if let EntryOutput::Field(val) = output {
        assert_eq!(val, "alice");
    } else {
        panic!("expected Field variant");
    }
}

#[tokio::test]
async fn get_unknown_field_errors() {
    let dir = tempfile::tempdir().unwrap();
    let (registry, identity, _vault) = make_registry_single(&dir.path().join("test.db")).await;
    let cmd = EntryCommands::Get {
        name: "GitHub".to_string(),
        field: Some("does-not-exist".to_string()),
        vault: None,
    };
    let result = handle_get(cmd, registry, &identity).await;
    assert!(result.is_err());
}

// ── search ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn search_returns_matching_entries() {
    let dir = tempfile::tempdir().unwrap();
    let (registry, identity, _vault) = make_registry_multi(&dir.path().join("test.db")).await;
    let cmd = EntryCommands::Search {
        query: "git".to_string(),
        vault: None,
    };
    let EntryOutput::List(entries) = handle_search(cmd, registry, &identity).await.unwrap() else {
        panic!("expected List variant");
    };
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e| e.name == "GitHub"));
    assert!(entries.iter().any(|e| e.name == "Gitlab"));
}

#[tokio::test]
async fn search_no_match_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let (registry, identity, _vault) = make_registry_multi(&dir.path().join("test.db")).await;
    let cmd = EntryCommands::Search {
        query: "zzz-no-match".to_string(),
        vault: None,
    };
    let EntryOutput::List(entries) = handle_search(cmd, registry, &identity).await.unwrap() else {
        panic!("expected List variant");
    };
    assert!(entries.is_empty());
}

#[tokio::test]
async fn search_named_vault() {
    let dir = tempfile::tempdir().unwrap();
    let (registry, identity, _vault) = make_registry_multi(&dir.path().join("test.db")).await;
    let cmd = EntryCommands::Search {
        query: "AWS".to_string(),
        vault: Some("Personal".to_string()),
    };
    let EntryOutput::List(entries) = handle_search(cmd, registry, &identity).await.unwrap() else {
        panic!("expected List variant");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "AWS");
}

#[tokio::test]
async fn search_unknown_vault_errors() {
    let dir = tempfile::tempdir().unwrap();
    let (registry, identity, _vault) = make_registry_multi(&dir.path().join("test.db")).await;
    let cmd = EntryCommands::Search {
        query: "anything".to_string(),
        vault: Some("nonexistent".to_string()),
    };
    let result = handle_search(cmd, registry, &identity).await;
    assert!(result.is_err());
}
