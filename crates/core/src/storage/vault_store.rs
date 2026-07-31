//! VaultStore implementation for SQLite storage.

use super::mapper::row_to_vault;
use super::sqlite::SqliteStorage;
use crate::domain::{Vault, VaultBackend};
use crate::error::{DbError, Error, Result};
use crate::ports::VaultStore;
use async_trait::async_trait;
use serde_json;

#[async_trait]
impl<C> VaultStore for SqliteStorage<C>
where
    C: crate::ports::Crypto + Send + Sync,
{
    async fn save_vault(&self, vault: &Vault) -> Result<()> {
        let backend_type = match &vault.backend {
            VaultBackend::Git(_) => "git",
            VaultBackend::Aws(_) => "aws",
            VaultBackend::Gcp(_) => "gcp",
            VaultBackend::Sqlite(_) => "sqlite",
        };
        let backend_config = serde_json::to_string(&vault.backend)
            .map_err(|e| Error::Database(DbError::Query(e.to_string())))?;
        let recipients = serde_json::to_string(&vault.recipients)
            .map_err(|e| Error::Database(DbError::Query(e.to_string())))?;

        sqlx::query(
            r#"
            INSERT INTO vaults (id, name, is_default, created_at, backend_type, backend_config, recipients, lock_timeout, auto_sync)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
                name         = excluded.name,
                is_default   = excluded.is_default,
                backend_type = excluded.backend_type,
                backend_config = excluded.backend_config,
                recipients   = excluded.recipients,
                lock_timeout = excluded.lock_timeout,
                auto_sync    = excluded.auto_sync
            "#,
        )
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
        let row = sqlx::query(
            r#"
            SELECT id, name, is_default, created_at, backend_config, recipients, lock_timeout, auto_sync
            FROM vaults
            WHERE id = ?1
            "#,
        )
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
        let rows = sqlx::query(
            r#"
            SELECT id, name, is_default, created_at, backend_config, recipients, lock_timeout, auto_sync
            FROM vaults
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DbError::Query(e.to_string()))?;

        rows.into_iter().map(row_to_vault).collect()
    }

    async fn default_vault(&self) -> Result<Option<Vault>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, is_default, created_at, backend_config, recipients, lock_timeout, auto_sync
            FROM vaults
            WHERE is_default = 1
            LIMIT 1
            "#,
        )
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
