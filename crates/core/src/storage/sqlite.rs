//! SQLite storage adapter implementation
//!
//! Implements the Storage port trait using SQLite with field-level encryption.

use crate::domain::{AgeIdentity, AgeRecipient, FieldValue};
use crate::error::{DbError, Error, Result};
use crate::ports::Crypto;
use base64::{engine::general_purpose, Engine as _};
use serde_json;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

/// Busy timeout used by the long-lived agent daemon connection.
const DAEMON_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// SQLite storage adapter with field-level encryption
#[derive(Clone)]
pub struct SqliteStorage<C>
where
    C: Crypto,
{
    /// Adapter-internal connection pool — siblings in `storage` may borrow it.
    pub(crate) pool: SqlitePool,
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
        Self::open(db_path, crypto, |options| options).await
    }

    /// Create the adapter for the agent daemon's long-lived connection.
    ///
    /// Enables WAL journaling and a 5 s busy timeout so the daemon's reads
    /// coexist with TUI writers instead of failing with `SQLITE_BUSY`. Both
    /// settings are per-connection pragmas: no schema or migration change.
    pub async fn new_daemon(db_path: impl AsRef<Path>, crypto: C) -> Result<Self> {
        Self::open(db_path, crypto, |options| {
            options
                .journal_mode(SqliteJournalMode::Wal)
                .busy_timeout(DAEMON_BUSY_TIMEOUT)
        })
        .await
    }

    async fn open(
        db_path: impl AsRef<Path>,
        crypto: C,
        tune: impl FnOnce(SqliteConnectOptions) -> SqliteConnectOptions,
    ) -> Result<Self> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = db_path.as_ref().parent() {
            crate::storage::config::ensure_private_dir(parent)?;
        }

        // Connect to SQLite database
        let options =
            SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.as_ref().display()))
                .map_err(|e| DbError::ConnectionFailed(e.to_string()))?
                .create_if_missing(true);
        let options = tune(options);

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

    /// Create an in-memory SQLite storage adapter (for fallback / testing).
    pub fn new_in_memory(crypto: C) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_lazy("sqlite::memory:")
            .map_err(|e| DbError::ConnectionFailed(e.to_string()))?;
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
}

// region:    --- Tests
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/storage/sqlite/test.rs"]
mod test;
// endregion: --- Tests
