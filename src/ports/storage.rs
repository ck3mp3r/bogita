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
    /// Save an entry to storage with encryption
    ///
    /// Creates a new entry if id doesn't exist, updates if it does.
    /// Recipients are used to encrypt fields marked as encrypted.
    async fn save_entry(
        &self,
        entry: &Entry,
        recipients: &[crate::domain::AgeRecipient],
    ) -> Result<()>;

    /// Retrieve an entry by ID with decryption
    ///
    /// Returns None if entry doesn't exist.
    /// Identity is used to decrypt fields marked as encrypted.
    async fn get_entry(
        &self,
        id: Uuid,
        identity: &crate::domain::AgeIdentity,
    ) -> Result<Option<Entry>>;

    /// List all entries in a vault
    ///
    /// Returns entries matching the given vault_id.
    /// Identity is used to decrypt fields marked as encrypted.
    async fn list_entries(
        &self,
        vault_id: Uuid,
        identity: &crate::domain::AgeIdentity,
    ) -> Result<Vec<Entry>>;

    /// Delete an entry by ID
    ///
    /// Returns error if entry doesn't exist.
    async fn delete_entry(&self, id: Uuid) -> Result<()>;

    /// Search entries by metadata
    ///
    /// Searches across plaintext fields only (encrypted fields are not searchable).
    /// Returns entries where any plaintext field contains the query string.
    /// Identity is used to decrypt fields marked as encrypted in results.
    async fn search_entries(
        &self,
        vault_id: Uuid,
        query: &str,
        identity: &crate::domain::AgeIdentity,
    ) -> Result<Vec<Entry>>;
}
