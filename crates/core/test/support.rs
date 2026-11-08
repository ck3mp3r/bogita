//! Shared test helpers for bogita-core.
//!
//! Provides [`MockKeychain`], an in-memory implementation of [`KeychainStore`]
//! that avoids touching the OS keychain. All test code should use this instead
//! of the `keyring-core` mock store.
//!
//! Also provides [`seed_vault`], a raw vault-row insert so tests can satisfy
//! foreign-key constraints without building a full `Vault`.

use crate::ports::KeychainStore;
use std::sync::Mutex;

#[cfg(feature = "integration")]
use crate::error::{DbError, Error, Result};
#[cfg(feature = "integration")]
use crate::ports::Crypto;
#[cfg(feature = "integration")]
use crate::storage::sqlite::SqliteStorage;
#[cfg(feature = "integration")]
use uuid::Uuid;

/// In-memory keychain for testing. Never touches the OS keychain.
pub struct MockKeychain {
    inner: Mutex<Option<String>>,
}

impl MockKeychain {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

impl Default for MockKeychain {
    fn default() -> Self {
        Self::new()
    }
}

impl KeychainStore for MockKeychain {
    fn store_identity(&self, secret_key: &str) -> crate::error::Result<()> {
        *self.inner.lock().expect("mock keychain mutex poisoned") = Some(secret_key.to_string());
        Ok(())
    }

    fn get_identity(&self) -> crate::error::Result<Option<String>> {
        Ok(self
            .inner
            .lock()
            .expect("mock keychain mutex poisoned")
            .clone())
    }

    fn delete_identity(&self) -> crate::error::Result<()> {
        *self.inner.lock().expect("mock keychain mutex poisoned") = None;
        Ok(())
    }
}

/// Insert a minimal vault row so FK constraints pass in tests.
///
/// Bypasses the vault API on purpose: tests need a parent row without
/// constructing a full `Vault` with recipients.
#[cfg(feature = "integration")]
pub async fn seed_vault<C: Crypto>(storage: &SqliteStorage<C>, vault_id: Uuid) -> Result<()> {
    sqlx::query(
        "INSERT INTO vaults (id, name, is_default, created_at, backend_type, backend_config, recipients, auto_sync)
             VALUES (?, ?, 0, ?, 'none', '{}', '[]', 0)",
    )
    .bind(vault_id.to_string())
    .bind(vault_id.to_string()) // use id as name to keep it unique
    .bind(chrono::Utc::now().timestamp())
    .execute(&storage.pool)
    .await
    .map_err(|e| Error::Database(DbError::Query(e.to_string())))?;
    Ok(())
}
