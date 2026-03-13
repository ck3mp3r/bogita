//! SQLite storage adapter implementation
//!
//! Implements the Storage port trait using SQLite with field-level encryption.

use crate::domain::{AgeIdentity, AgeRecipient, Entry, EntryType, Field, FieldType, FieldValue};
use crate::error::{DbError, Error, Result};
use crate::ports::{Crypto, Storage};
use base64::{engine::general_purpose, Engine as _};
use serde_json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use std::path::Path;
use std::str::FromStr;

/// SQLite storage adapter with field-level encryption
pub struct SqliteStorage<C>
where
    C: Crypto,
{
    pool: SqlitePool,
    crypto: C,
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
    fn encrypt_field_value(
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
    fn decrypt_field_value(
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
}

#[async_trait::async_trait]
impl<C> Storage for SqliteStorage<C>
where
    C: Crypto + Send + Sync,
{
    async fn save_entry(&self, entry: &Entry, recipients: &[AgeRecipient]) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::Database(DbError::Query(e.to_string())))?;

        // Insert or replace entry
        sqlx::query(
            r#"
            INSERT INTO entries (id, vault_id, name, entry_type, created_at, modified_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                entry_type = excluded.entry_type,
                modified_at = excluded.modified_at
            "#,
        )
        .bind(entry.id.to_string())
        .bind(entry.vault_id.to_string())
        .bind(&entry.name)
        .bind(match entry.entry_type {
            EntryType::Password => "password",
            EntryType::Otp => "otp",
            EntryType::SshKey => "ssh_key",
            EntryType::Note => "note",
        })
        .bind(entry.created_at)
        .bind(entry.modified_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(DbError::Query(e.to_string())))?;

        // Delete old fields for this entry
        sqlx::query("DELETE FROM entry_fields WHERE entry_id = ?1")
            .bind(entry.id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Database(DbError::Query(e.to_string())))?;

        // Insert new fields
        for field in &entry.fields {
            let value_str = self.encrypt_field_value(&field.value, field.encrypted, recipients)?;

            let field_type_str = match &field.field_type {
                FieldType::Username => "username".to_string(),
                FieldType::Password => "password".to_string(),
                FieldType::Url => "url".to_string(),
                FieldType::Notes => "notes".to_string(),
                FieldType::Tags => "tags".to_string(),
                FieldType::Favorite => "favorite".to_string(),
                FieldType::TotpSecret => "totp_secret".to_string(),
                FieldType::TotpAlgorithm => "totp_algorithm".to_string(),
                FieldType::TotpDigits => "totp_digits".to_string(),
                FieldType::TotpPeriod => "totp_period".to_string(),
                FieldType::TotpIssuer => "totp_issuer".to_string(),
                FieldType::TotpAccount => "totp_account".to_string(),
                FieldType::SshPrivateKey => "ssh_private_key".to_string(),
                FieldType::SshPublicKey => "ssh_public_key".to_string(),
                FieldType::SshKeyType => "ssh_key_type".to_string(),
                FieldType::SshComment => "ssh_comment".to_string(),
                FieldType::PasswordHistory => "password_history".to_string(),
                FieldType::Custom(name) => format!("custom:{}", name),
            };

            sqlx::query(
                r#"
                INSERT INTO entry_fields (id, entry_id, key, value, field_type, encrypted, idx)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
            )
            .bind(field.id.to_string())
            .bind(entry.id.to_string())
            .bind(&field.key)
            .bind(value_str)
            .bind(field_type_str)
            .bind(field.encrypted)
            .bind(field.idx)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Database(DbError::Query(e.to_string())))?;
        }

        tx.commit()
            .await
            .map_err(|e| Error::Database(DbError::Query(e.to_string())))?;
        Ok(())
    }

    async fn get_entry(&self, id: uuid::Uuid, identity: &AgeIdentity) -> Result<Option<Entry>> {
        // Get entry
        let entry_row = sqlx::query(
            r#"
            SELECT id, vault_id, name, entry_type, created_at, modified_at
            FROM entries
            WHERE id = ?1
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DbError::Query(e.to_string()))?;

        let entry_row = match entry_row {
            Some(row) => row,
            None => return Ok(None),
        };

        // Get fields for this entry
        let field_rows = sqlx::query(
            r#"
            SELECT id, key, value, field_type, encrypted, idx
            FROM entry_fields
            WHERE entry_id = ?1
            ORDER BY idx
            "#,
        )
        .bind(id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DbError::Query(e.to_string()))?;

        // Reconstruct fields
        let mut fields = Vec::new();
        for row in field_rows {
            let field_id: String = row.get("id");
            let key: String = row.get("key");
            let value_str: String = row.get("value");
            let field_type_str: String = row.get("field_type");
            let encrypted: bool = row.get("encrypted");
            let idx: i32 = row.get("idx");

            let value = self.decrypt_field_value(&value_str, encrypted, identity)?;

            // Parse field_type from string
            let field_type = if field_type_str.starts_with("custom:") {
                FieldType::Custom(field_type_str.strip_prefix("custom:").unwrap().to_string())
            } else {
                match field_type_str.as_str() {
                    "username" => FieldType::Username,
                    "password" => FieldType::Password,
                    "url" => FieldType::Url,
                    "notes" => FieldType::Notes,
                    "tags" => FieldType::Tags,
                    "favorite" => FieldType::Favorite,
                    "totp_secret" => FieldType::TotpSecret,
                    "totp_algorithm" => FieldType::TotpAlgorithm,
                    "totp_digits" => FieldType::TotpDigits,
                    "totp_period" => FieldType::TotpPeriod,
                    "totp_issuer" => FieldType::TotpIssuer,
                    "totp_account" => FieldType::TotpAccount,
                    "ssh_private_key" => FieldType::SshPrivateKey,
                    "ssh_public_key" => FieldType::SshPublicKey,
                    "ssh_key_type" => FieldType::SshKeyType,
                    "ssh_comment" => FieldType::SshComment,
                    "password_history" => FieldType::PasswordHistory,
                    other => FieldType::Custom(other.to_string()),
                }
            };

            fields.push(Field {
                id: uuid::Uuid::parse_str(&field_id).map_err(|_| DbError::CorruptedData)?,
                key,
                value,
                field_type,
                encrypted,
                idx,
            });
        }

        // Parse entry_type from string
        let entry_type_str: String = entry_row.get("entry_type");
        let entry_type = match entry_type_str.as_str() {
            "password" => EntryType::Password,
            "otp" => EntryType::Otp,
            "ssh_key" => EntryType::SshKey,
            "note" => EntryType::Note,
            _ => return Err(DbError::CorruptedData.into()),
        };

        Ok(Some(Entry {
            id: uuid::Uuid::parse_str(&entry_row.get::<String, _>("id"))
                .map_err(|_| DbError::CorruptedData)?,
            vault_id: uuid::Uuid::parse_str(&entry_row.get::<String, _>("vault_id"))
                .map_err(|_| DbError::CorruptedData)?,
            name: entry_row.get("name"),
            entry_type,
            created_at: entry_row.get("created_at"),
            modified_at: entry_row.get("modified_at"),
            fields,
        }))
    }

    async fn list_entries(
        &self,
        vault_id: uuid::Uuid,
        identity: &AgeIdentity,
    ) -> Result<Vec<Entry>> {
        // Get all entries for vault
        let entry_rows = sqlx::query(
            r#"
            SELECT id
            FROM entries
            WHERE vault_id = ?1
            ORDER BY name
            "#,
        )
        .bind(vault_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DbError::Query(e.to_string()))?;

        let mut entries = Vec::new();
        for row in entry_rows {
            let id: String = row.get("id");
            let entry_id = uuid::Uuid::parse_str(&id).map_err(|_| DbError::CorruptedData)?;

            if let Some(entry) = self.get_entry(entry_id, identity).await? {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    async fn delete_entry(&self, id: uuid::Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM entries WHERE id = ?1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Query(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DbError::EntryNotFound(id).into());
        }

        Ok(())
    }

    async fn search_entries(
        &self,
        vault_id: uuid::Uuid,
        query: &str,
        identity: &AgeIdentity,
    ) -> Result<Vec<Entry>> {
        // Search in plaintext fields only
        let search_pattern = format!("%{}%", query);

        let entry_rows = sqlx::query(
            r#"
            SELECT DISTINCT e.id
            FROM entries e
            JOIN entry_fields f ON f.entry_id = e.id
            WHERE e.vault_id = ?1
              AND f.encrypted = 0
              AND (f.key LIKE ?2 OR f.value LIKE ?2 OR e.name LIKE ?2)
            ORDER BY e.name
            "#,
        )
        .bind(vault_id.to_string())
        .bind(&search_pattern)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DbError::Query(e.to_string()))?;

        let mut entries = Vec::new();
        for row in entry_rows {
            let id: String = row.get("id");
            let entry_id = uuid::Uuid::parse_str(&id).map_err(|_| DbError::CorruptedData)?;

            if let Some(entry) = self.get_entry(entry_id, identity).await? {
                entries.push(entry);
            }
        }

        Ok(entries)
    }
}
