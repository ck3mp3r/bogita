use crate::args::VaultCommands;
use crate::handlers::vault::{handle_vault, VaultOutput};
use bogita_core::crypto::AgeCrypto;
use bogita_core::domain::{AgeIdentity, Vault};
use bogita_core::storage::sqlite::SqliteStorage;
use bogita_core::vault::registry::VaultRegistry;
use chrono::Utc;
use uuid::Uuid;

async fn make_registry(
    db_path: &std::path::Path,
) -> VaultRegistry<SqliteStorage<AgeCrypto>, AgeCrypto> {
    let storage = SqliteStorage::new(db_path, AgeCrypto).await.unwrap();
    let registry = VaultRegistry::new(storage, AgeCrypto);
    let identity = AgeIdentity::generate();
    let vault = Vault {
        id: Uuid::new_v4(),
        name: "Personal".to_string(),
        is_default: true,
        created_at: Utc::now().timestamp(),
        sync_target: None,
        recipients: vec![identity.to_recipient()],
        lock_timeout: None,
        auto_sync: false,
    };
    registry.add_vault(&vault).await.unwrap();
    registry
}

#[tokio::test]
async fn vault_list_returns_vaults() {
    let dir = tempfile::tempdir().unwrap();
    let registry = make_registry(&dir.path().join("test.db")).await;
    let output = handle_vault(VaultCommands::List, registry).await.unwrap();
    assert!(matches!(output, VaultOutput::List(ref v) if v.len() == 1 && v[0].name == "Personal"));
}

#[tokio::test]
async fn vault_default_sets_default() {
    let dir = tempfile::tempdir().unwrap();
    let registry = make_registry(&dir.path().join("test.db")).await;
    let output = handle_vault(
        VaultCommands::Default {
            name: "Personal".to_string(),
        },
        registry,
    )
    .await
    .unwrap();
    assert!(matches!(output, VaultOutput::Ok));
}

#[tokio::test]
async fn vault_default_unknown_vault_errors() {
    let dir = tempfile::tempdir().unwrap();
    let registry = make_registry(&dir.path().join("test.db")).await;
    let result = handle_vault(
        VaultCommands::Default {
            name: "nonexistent".to_string(),
        },
        registry,
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn vault_rm_removes_vault() {
    let dir = tempfile::tempdir().unwrap();
    let registry = make_registry(&dir.path().join("test.db")).await;
    let output = handle_vault(
        VaultCommands::Rm {
            name: "Personal".to_string(),
        },
        registry.clone(),
    )
    .await
    .unwrap();
    assert!(matches!(output, VaultOutput::Ok));
    // Verify vault is gone
    let vaults = registry.list_vaults().await.unwrap();
    assert!(vaults.is_empty());
}

#[tokio::test]
async fn vault_rm_unknown_errors() {
    let dir = tempfile::tempdir().unwrap();
    let registry = make_registry(&dir.path().join("test.db")).await;
    let result = handle_vault(
        VaultCommands::Rm {
            name: "nonexistent".to_string(),
        },
        registry,
    )
    .await;
    assert!(result.is_err());
}
