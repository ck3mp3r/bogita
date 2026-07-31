//! SQLite storage adapter implementation
//!
//! Implements the Storage port trait using SQLite with field-level encryption.

use crate::domain::{AgeIdentity, AgeRecipient, FieldValue};
use crate::error::{DbError, Error, Result};
use crate::ports::Crypto;
use base64::{engine::general_purpose, Engine as _};
use serde_json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

/// SQLite storage adapter with field-level encryption
#[derive(Clone)]
pub struct SqliteStorage<C>
where
    C: Crypto,
{
    pub(crate) pool: SqlitePool,
    pub(crate) crypto: C,
}

impl<C> SqliteStorage<C>
where
    C: Crypto,
{
    /// Create a new SQLite storage adapter
    ///
    /// Creates the database file and runs migrations if needed.
    pub async fn new(db_path: impl AsRef<Path>, crypto: C) -> Result<Self> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = db_path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Connect to SQLite database
        let options =
            SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.as_ref().display()))
                .map_err(|e| DbError::ConnectionFailed(e.to_string()))?
                .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|e| DbError::ConnectionFailed(e.to_string()))?;

        // Run migrations
        sqlx::migrate!("data/sql/sqlite/migrations")
            .run(&pool)
            .await
            .map_err(|e| DbError::MigrationFailed(e.to_string()))?;

        Ok(Self { pool, crypto })
    }

    /// Encrypt a field value if the field is marked as encrypted
    pub(crate) fn encrypt_field_value(
        &self,
        value: &FieldValue,
        encrypted: bool,
        recipients: &[AgeRecipient],
    ) -> Result<String> {
        if !encrypted {
            // Plaintext field - just serialize to JSON
            Ok(serde_json::to_string(value)
                .map_err(|e| Error::Database(DbError::Query(e.to_string())))?)
        } else {
            // Encrypted field - serialize then encrypt with age
            let json_bytes = serde_json::to_vec(value)
                .map_err(|e| Error::Database(DbError::Query(e.to_string())))?;
            let encrypted_bytes = self.crypto.encrypt(&json_bytes, recipients)?;
            // Encode as base64 for storage
            Ok(general_purpose::STANDARD.encode(&encrypted_bytes))
        }
    }

    /// Decrypt a field value if the field is marked as encrypted
    pub(crate) fn decrypt_field_value(
        &self,
        value_str: &str,
        encrypted: bool,
        identity: &AgeIdentity,
    ) -> Result<FieldValue> {
        if !encrypted {
            // Plaintext field - just deserialize from JSON
            Ok(serde_json::from_str(value_str)
                .map_err(|e| Error::Database(DbError::Query(e.to_string())))?)
        } else {
            // Encrypted field - decode base64 then decrypt with age
            let encrypted_bytes = general_purpose::STANDARD
                .decode(value_str)
                .map_err(|_| Error::Database(DbError::CorruptedData))?;
            let decrypted_bytes = self.crypto.decrypt(&encrypted_bytes, identity)?;
            Ok(serde_json::from_slice(&decrypted_bytes)
                .map_err(|e| Error::Database(DbError::Query(e.to_string())))?)
        }
    }

    /// Insert a minimal vault row for use in tests (bypasses FK constraints)
    #[cfg(test)]
    pub(crate) async fn seed_vault_for_test(&self, vault_id: uuid::Uuid) -> Result<()> {
        sqlx::query(
            "INSERT INTO vaults (id, name, is_default, created_at, backend_type, backend_config, recipients, auto_sync)
             VALUES (?, ?, 0, ?, 'sqlite', '{}', '[]', 0)",
        )
        .bind(vault_id.to_string())
        .bind(vault_id.to_string()) // use id as name to keep it unique
        .bind(chrono::Utc::now().timestamp())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Database(DbError::Query(e.to_string())))?;
        Ok(())
    }
}
