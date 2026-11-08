use bogita_core::ports::KeychainStore;
use std::sync::Mutex;

pub(super) struct MockKeychain {
    inner: Mutex<Option<String>>,
}

impl MockKeychain {
    fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

impl KeychainStore for MockKeychain {
    fn store_identity(&self, secret_key: &str) -> bogita_core::error::Result<()> {
        *self.inner.lock().expect("mock keychain mutex poisoned") = Some(secret_key.to_string());
        Ok(())
    }

    fn get_identity(&self) -> bogita_core::error::Result<Option<String>> {
        Ok(self
            .inner
            .lock()
            .expect("mock keychain mutex poisoned")
            .clone())
    }

    fn delete_identity(&self) -> bogita_core::error::Result<()> {
        *self.inner.lock().expect("mock keychain mutex poisoned") = None;
        Ok(())
    }
}

/// Build a minimal App for testing without touching the filesystem.
///
/// If `with_entry` is true, seeds the Personal vault with one entry.
pub(super) async fn make_app() -> bogita_core::app::App<MockKeychain> {
    use bogita_core::crypto::AgeCrypto;
    use bogita_core::domain::{AgeIdentity, Vault};
    use bogita_core::session::Session;
    use bogita_core::storage::sqlite::SqliteStorage;
    use bogita_core::vault::registry::VaultRegistry;
    use chrono::Utc;
    use uuid::Uuid;

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("tui_test.db");
    let identity = AgeIdentity::generate();
    let storage = SqliteStorage::new(&db_path, AgeCrypto).await.unwrap();
    let registry = VaultRegistry::new(storage, AgeCrypto);
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

    // Leak the tempdir so the DB file lives for the duration of the test.
    std::mem::forget(dir);

    bogita_core::app::App {
        config: bogita_core::storage::config::AppConfig::default(),
        identity: Some(identity),
        registry,
        session: Session::new(MockKeychain::new()),
        is_locked: false,
        lock_timeout: None,
    }
}

/// Build an App seeded with one entry in the Personal vault.
pub(super) async fn make_app_with_entry() -> bogita_core::app::App<MockKeychain> {
    use bogita_core::domain::{Entry, EntryType, Field, FieldType, FieldValue};
    use chrono::Utc;
    use uuid::Uuid;

    let app = make_app().await;
    let vaults = app.registry.list_vaults().await.unwrap();
    let vault = vaults.first().unwrap();
    let svc = app
        .registry
        .vault_service_for(vault, app.identity.as_ref().unwrap().clone());
    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: vault.id,
        name: "GitHub".to_string(),
        entry_type: EntryType::Token,
        fields: vec![Field {
            id: Uuid::new_v4(),
            key: "password".to_string(),
            field_type: FieldType::Token,
            value: FieldValue::Hidden("s3cret".to_string()),
            encrypted: true,
            idx: 0,
        }],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    };
    svc.add_entry(&entry).await.unwrap();
    app
}
