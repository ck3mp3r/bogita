//! Storage factory port — opens the vault database for an `App`.

use crate::error::Result;
use crate::ports::{EntryStore, VaultStore};
use crate::storage::sqlite::SqliteStorage;
use async_trait::async_trait;
use std::path::Path;

/// Opens the storage an [`crate::app::App`] runs on.
///
/// The app only knows this trait, so tests can inject storage that never
/// touches disk. Production uses [`SqliteStorageFactory`].
#[async_trait]
pub trait VaultStorageFactory: Send + Sync {
    /// Storage adapter produced by this factory.
    type Storage: VaultStore + EntryStore + Send + Sync;

    /// Open (creating it when needed) the vault database at `db_path`.
    async fn open(&self, db_path: &Path) -> Result<Self::Storage>;

    /// Open an in-memory fallback used when [`Self::open`] fails.
    fn open_in_memory(&self) -> Result<Self::Storage>;
}

/// SQLite-backed factory used by the production composition root.
#[derive(Clone, Copy, Default)]
pub struct SqliteStorageFactory;

#[async_trait]
impl VaultStorageFactory for SqliteStorageFactory {
    type Storage = SqliteStorage<crate::crypto::age::AgeCrypto>;

    async fn open(&self, db_path: &Path) -> Result<Self::Storage> {
        SqliteStorage::new(db_path, crate::crypto::age::AgeCrypto).await
    }

    fn open_in_memory(&self) -> Result<Self::Storage> {
        SqliteStorage::new_in_memory(crate::crypto::age::AgeCrypto)
    }
}
