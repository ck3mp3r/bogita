//! Storage port trait
//!
//! Defines the interface for entry storage backends.

use crate::domain::{AgeIdentity, AgeRecipient, Entry, Uuid, Vault};
use crate::error::Result;
use async_trait::async_trait;

/// Storage port for persisting vault entries and vault metadata
///
/// This trait defines the interface that all storage adapters must implement.
/// It follows hexagonal architecture - the core defines the port, adapters implement it.
#[async_trait]
pub trait Storage: Send + Sync {
    // -----------------------------------------------------------------------
    // Vault metadata
    // -----------------------------------------------------------------------

    /// Persist vault metadata.
    ///
    /// Creates a new vault if the id doesn't exist, updates if it does.
    async fn save_vault(&self, vault: &Vault) -> Result<()>;

    /// Retrieve vault metadata by ID.
    ///
    /// Returns None if no vault with that id exists.
    async fn get_vault(&self, id: Uuid) -> Result<Option<Vault>>;

    /// List all persisted vaults.
    async fn list_vaults(&self) -> Result<Vec<Vault>>;

    /// Return the vault marked `is_default = true`, or `None` if none is set.
    async fn default_vault(&self) -> Result<Option<Vault>>;

    /// Delete a vault by ID.
    ///
    /// Returns error if the vault doesn't exist.
    async fn delete_vault(&self, id: Uuid) -> Result<()>;

    // -----------------------------------------------------------------------
    // Entry CRUD
    // -----------------------------------------------------------------------

    /// Save an entry to storage with encryption
    ///
    /// Creates a new entry if id doesn't exist, updates if it does.
    /// Recipients are used to encrypt fields marked as encrypted.
    async fn save_entry(&self, entry: &Entry, recipients: &[AgeRecipient]) -> Result<()>;

    /// Retrieve an entry by ID with decryption
    ///
    /// Returns None if entry doesn't exist.
    /// Identity is used to decrypt fields marked as encrypted.
    async fn get_entry(&self, id: Uuid, identity: &AgeIdentity) -> Result<Option<Entry>>;

    /// List all entries in a vault, with optional search filter
    ///
    /// Returns entries matching the given vault_id.
    /// When query is Some, filters to entries where any plaintext field contains the query.
    /// When query is None, returns all entries.
    /// Identity is used to decrypt fields marked as encrypted.
    async fn list_entries(
        &self,
        vault_id: Uuid,
        query: Option<&str>,
        identity: &AgeIdentity,
    ) -> Result<Vec<Entry>>;

    /// Delete an entry by ID
    ///
    /// Returns error if entry doesn't exist.
    async fn delete_entry(&self, id: Uuid) -> Result<()>;
}

/// Forward all Storage calls through a shared reference.
///
/// This allows `VaultRegistry` to lend `&self.storage` to `VaultService`
/// without moving or cloning the underlying adapter.
#[async_trait]
impl<S: Storage> Storage for &S {
    async fn save_vault(&self, vault: &Vault) -> Result<()> {
        (**self).save_vault(vault).await
    }
    async fn get_vault(&self, id: Uuid) -> Result<Option<Vault>> {
        (**self).get_vault(id).await
    }
    async fn list_vaults(&self) -> Result<Vec<Vault>> {
        (**self).list_vaults().await
    }
    async fn default_vault(&self) -> Result<Option<Vault>> {
        (**self).default_vault().await
    }
    async fn delete_vault(&self, id: Uuid) -> Result<()> {
        (**self).delete_vault(id).await
    }
    async fn save_entry(&self, entry: &Entry, recipients: &[AgeRecipient]) -> Result<()> {
        (**self).save_entry(entry, recipients).await
    }
    async fn get_entry(&self, id: Uuid, identity: &AgeIdentity) -> Result<Option<Entry>> {
        (**self).get_entry(id, identity).await
    }
    async fn list_entries(
        &self,
        vault_id: Uuid,
        query: Option<&str>,
        identity: &AgeIdentity,
    ) -> Result<Vec<Entry>> {
        (**self).list_entries(vault_id, query, identity).await
    }
    async fn delete_entry(&self, id: Uuid) -> Result<()> {
        (**self).delete_entry(id).await
    }
}
