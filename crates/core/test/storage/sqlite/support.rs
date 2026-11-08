//! Shared fixtures for the SQLite storage tests.

use crate::crypto::age::AgeCrypto;
use crate::domain::{AgeIdentity, AgeRecipient, Entry, Field, FieldType, FieldValue};
use crate::storage::sqlite::SqliteStorage;
use crate::test_support::{test_entry, test_field};
use uuid::Uuid;

pub(super) use crate::test_support::test_vault_locked as make_vault;

pub(super) fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

pub(super) async fn create_test_storage() -> crate::error::Result<SqliteStorage<AgeCrypto>> {
    SqliteStorage::new(":memory:", AgeCrypto).await
}

pub(super) fn create_test_keys() -> (Vec<AgeRecipient>, AgeIdentity) {
    let identity = AgeIdentity::generate();
    let recipient = identity.to_recipient();
    (vec![recipient], identity)
}

/// Insert a minimal vault row so FK constraints pass during tests
pub(super) async fn seed_vault(
    storage: &SqliteStorage<AgeCrypto>,
    vault_id: Uuid,
) -> crate::error::Result<()> {
    crate::test_support::seed_vault(storage, vault_id).await
}

pub(super) fn make_entry(vault_id: Uuid, name: &str, fields: Vec<Field>) -> Entry {
    test_entry(vault_id, name, fields)
}

pub(super) fn plain_field(key: &str, value: &str, idx: i32) -> Field {
    test_field(
        key,
        FieldValue::Text(value.to_string()),
        FieldType::Username,
        false,
        idx,
    )
}

pub(super) fn secret_field(key: &str, value: &str, idx: i32) -> Field {
    test_field(
        key,
        FieldValue::Hidden(value.to_string()),
        FieldType::Token,
        true,
        idx,
    )
}
