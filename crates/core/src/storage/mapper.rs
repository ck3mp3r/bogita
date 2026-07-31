//! Row mapping functions for SQLite storage.

use crate::domain::{AgeRecipient, SyncTarget, Vault};
use crate::error::{DbError, Error, Result};
use sqlx::Row;

/// Convert a SQLite row into a `Vault` domain type.
pub(crate) fn row_to_vault(row: sqlx::sqlite::SqliteRow) -> Result<Vault> {
    let id: String = row.get("id");
    let name: String = row.get("name");
    let is_default: bool = row.get("is_default");
    let created_at: i64 = row.get("created_at");
    let backend_type: String = row.get("backend_type");
    let backend_config: String = row.get("backend_config");
    let recipients_json: String = row.get("recipients");
    let lock_timeout: Option<i64> = row.get("lock_timeout");
    let auto_sync: bool = row.get("auto_sync");

    let id = uuid::Uuid::parse_str(&id).map_err(|_| DbError::CorruptedData)?;
    let sync_target: Option<SyncTarget> = match backend_type.as_str() {
        "none" | "" | "sqlite" => None,
        _ => Some(
            serde_json::from_str(&backend_config)
                .map_err(|e| Error::Database(DbError::Query(e.to_string())))?,
        ),
    };
    let recipients: Vec<AgeRecipient> = serde_json::from_str(&recipients_json)
        .map_err(|e| Error::Database(DbError::Query(e.to_string())))?;

    Ok(Vault {
        id,
        name,
        is_default,
        created_at,
        sync_target,
        recipients,
        lock_timeout: lock_timeout.map(|t| t as u64),
        auto_sync,
    })
}
