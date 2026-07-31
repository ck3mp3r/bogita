use crate::cli::args::VaultCommands;
use crate::cli::handlers::vault::{handle_vault, VaultOutput};
use crate::crypto::age::AgeCrypto;
use crate::domain::{AgeIdentity, SqliteConfig, Vault, VaultBackend};
use crate::storage::sqlite::SqliteStorage;
use crate::vault::registry::VaultRegistry;
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
        backend: VaultBackend::Sqlite(SqliteConfig {
            path: db_path.to_string_lossy().to_string(),
        }),
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
