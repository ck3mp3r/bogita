//! VaultStore implementation for SQLite storage.

use super::mapper::row_to_vault;
use super::sqlite::SqliteStorage;
use crate::domain::{SyncTarget, Vault};
use crate::error::{DbError, Error, Result};
use crate::ports::VaultStore;
use async_trait::async_trait;
use serde_json;

/// Column list shared by every vault read and write.
///
/// A macro instead of a `const` so [`concat!`] can build compile-time `'static`
/// SQL strings — `sqlx::query` rejects runtime-built strings.
macro_rules! vault_columns {
    () => {
        "id, name, is_default, created_at, backend_type, backend_config, recipients, lock_timeout, auto_sync"
    };
}

#[async_trait]
impl<C> VaultStore for SqliteStorage<C>
where
    C: crate::ports::Crypto + Send + Sync,
{
    async fn save_vault(&self, vault: &Vault) -> Result<()> {
        let (backend_type, backend_config) = match &vault.sync_target {
            Some(sync_target) => (
                sync_backend_type(sync_target),
                serde_json::to_string(&vault.sync_target)
                    .map_err(|e| Error::Database(DbError::Query(e.to_string())))?,
            ),
            None => ("none", "{}".to_string()),
        };
        let recipients = serde_json::to_string(&vault.recipients)
            .map_err(|e| Error::Database(DbError::Query(e.to_string())))?;

        let insert_sql = concat!(
            "INSERT INTO vaults (",
            vault_columns!(),
            ")
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
                name         = excluded.name,
                is_default   = excluded.is_default,
                backend_type = excluded.backend_type,
                backend_config = excluded.backend_config,
                recipients   = excluded.recipients,
                lock_timeout = excluded.lock_timeout,
                auto_sync    = excluded.auto_sync
            "
        );
        sqlx::query(insert_sql)
            .bind(vault.id.to_string())
            .bind(&vault.name)
            .bind(vault.is_default)
            .bind(vault.created_at)
            .bind(backend_type)
            .bind(backend_config)
            .bind(recipients)
            .bind(vault.lock_timeout.map(|t| t as i64))
            .bind(vault.auto_sync)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Database(DbError::Query(e.to_string())))?;

        Ok(())
    }

    async fn get_vault(&self, id: uuid::Uuid) -> Result<Option<Vault>> {
        let row = sqlx::query(concat!(
            "SELECT ",
            vault_columns!(),
            " FROM vaults WHERE id = ?1"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DbError::Query(e.to_string()))?;

        match row {
            None => Ok(None),
            Some(r) => Ok(Some(row_to_vault(r)?)),
        }
    }

    async fn list_vaults(&self) -> Result<Vec<Vault>> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            vault_columns!(),
            " FROM vaults ORDER BY name"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DbError::Query(e.to_string()))?;

        rows.into_iter().map(row_to_vault).collect()
    }

    async fn default_vault(&self) -> Result<Option<Vault>> {
        let row = sqlx::query(concat!(
            "SELECT ",
            vault_columns!(),
            " FROM vaults WHERE is_default = 1 LIMIT 1"
        ))
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DbError::Query(e.to_string()))?;

        match row {
            None => Ok(None),
            Some(r) => Ok(Some(row_to_vault(r)?)),
        }
    }

    async fn delete_vault(&self, id: uuid::Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM vaults WHERE id = ?1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Query(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DbError::VaultNotFound(id).into());
        }

        Ok(())
    }
}

/// Storage discriminator written to `vaults.backend_type` for a sync target.
fn sync_backend_type(sync_target: &SyncTarget) -> &'static str {
    match sync_target {
        SyncTarget::Git(_) => "git",
        SyncTarget::Aws(_) => "aws",
        SyncTarget::Gcp(_) => "gcp",
    }
}
