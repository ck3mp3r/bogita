//! Storage port trait
//!
//! Defines the interface for entry storage backends.

use crate::domain::{Entry, Uuid};
use crate::error::Result;
use async_trait::async_trait;

/// Storage port for persisting vault entries
///
/// This trait defines the interface that all storage adapters must implement.
/// It follows hexagonal architecture - the core defines the port, adapters implement it.
#[async_trait]
pub trait Storage: Send + Sync {
    /// Save an entry to storage
    ///
    /// Creates a new entry if id doesn't exist, updates if it does.
    async fn save_entry(&self, entry: &Entry) -> Result<()>;

    /// Retrieve an entry by ID
    ///
    /// Returns None if entry doesn't exist.
    async fn get_entry(&self, id: Uuid) -> Result<Option<Entry>>;

    /// List all entries in a vault
    ///
    /// Returns entries matching the given vault_id.
    /// If vault_id is None, returns all entries across all vaults.
    async fn list_entries(&self, vault_id: Option<Uuid>) -> Result<Vec<Entry>>;

    /// Delete an entry by ID
    ///
    /// Returns Ok(()) even if entry doesn't exist (idempotent).
    async fn delete_entry(&self, id: Uuid) -> Result<()>;

    /// Search entries by metadata
    ///
    /// Searches across cleartext metadata fields (name, url, username, notes).
    /// Returns entries where any field contains the query string (case-insensitive).
    async fn search_entries(&self, vault_id: Option<Uuid>, query: &str) -> Result<Vec<Entry>>;
}
