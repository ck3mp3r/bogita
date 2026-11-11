//! VaultRegistry — manages multiple vaults, constructs VaultEntryService per vault.
//!
//! The pass-through methods (`add_vault`, `list_vaults`, `default_vault`,
//! `remove_vault`) are intentional facade surface: callers depend on the
//! registry, not on the storage adapter. Only `set_default` and
//! `vault_service_for` add behaviour (audit L2).

use crate::domain::{AgeIdentity, Entry, Vault};
use crate::error::{DbError, Result};
use crate::ports::{EntryStore, VaultStore};
use crate::vault::service::VaultEntryService;
use async_trait::async_trait;
use uuid::Uuid;

/// Registry that manages vault metadata and constructs a `VaultEntryService` per vault.
///
/// The registry owns the shared storage adapter. A `VaultEntryService` is
/// constructed on demand for a specific vault, bound to that vault's recipients
/// and the caller's identity.
#[derive(Clone)]
pub struct VaultRegistry<S>
where
    S: VaultStore,
{
    storage: S,
}

impl<S> VaultRegistry<S>
where
    S: VaultStore,
{
    pub fn new(storage: S) -> Self {
        Self { storage }
    }

    /// Persist a new vault (or update existing).
    pub async fn add_vault(&self, vault: &Vault) -> Result<()> {
        self.storage.save_vault(vault).await
    }

    /// Return all persisted vaults.
    pub async fn list_vaults(&self) -> Result<Vec<Vault>> {
        self.storage.list_vaults().await
    }

    /// Return the vault marked as default, or `None` if none is set.
    pub async fn default_vault(&self) -> Result<Option<Vault>> {
        self.storage.default_vault().await
    }

    /// Remove a vault by ID.
    pub async fn remove_vault(&self, id: Uuid) -> Result<()> {
        self.storage.delete_vault(id).await
    }

    /// Make `id` the default vault, clearing any previous default.
    ///
    /// Loads all vaults, flips the `is_default` flag, and persists each one.
    pub async fn set_default(&self, id: Uuid) -> Result<()> {
        let mut vaults = self.storage.list_vaults().await?;
        for vault in &mut vaults {
            vault.is_default = vault.id == id;
            self.storage.save_vault(vault).await?;
        }
        Ok(())
    }

    /// Construct a `VaultEntryService` scoped to the given vault and identity.
    ///
    /// The service uses the vault's recipients for encryption and the supplied
    /// identity for decryption. The registry's storage is borrowed by reference
    /// so no cloning of heavy state is required.
    pub fn vault_service_for(&self, vault: &Vault, identity: AgeIdentity) -> VaultEntryService<&S>
    where
        S: VaultStore + EntryStore,
    {
        VaultEntryService::new(&self.storage, vault.recipients.clone(), identity)
    }
}

// region:    --- Ports

/// The registry surface the vault service delegates to.
///
/// The service owns one registry and passes its held identity with each call, so
/// the registry stays the only owner of the storage adapter. Reads need the
/// identity to decrypt; writes need the vault recipients.
#[async_trait]
pub trait VaultAccess: Send + Sync {
    async fn list_vaults(&self) -> Result<Vec<Vault>>;

    async fn add_vault(&self, vault: &Vault) -> Result<()>;

    async fn remove_vault(&self, id: Uuid) -> Result<()>;

    async fn set_default(&self, id: Uuid) -> Result<()>;

    async fn list_entries(
        &self,
        vault_id: Uuid,
        query: Option<&str>,
        identity: &AgeIdentity,
    ) -> Result<Vec<Entry>>;

    async fn get_entry(&self, id: Uuid, identity: &AgeIdentity) -> Result<Option<Entry>>;

    async fn save_entry(&self, entry: &Entry, identity: &AgeIdentity) -> Result<()>;

    async fn delete_entry(&self, id: Uuid) -> Result<()>;
}

#[async_trait]
impl<S> VaultAccess for VaultRegistry<S>
where
    S: VaultStore + EntryStore,
{
    async fn list_vaults(&self) -> Result<Vec<Vault>> {
        self.storage.list_vaults().await
    }

    async fn add_vault(&self, vault: &Vault) -> Result<()> {
        self.storage.save_vault(vault).await
    }

    async fn remove_vault(&self, id: Uuid) -> Result<()> {
        self.storage.delete_vault(id).await
    }

    async fn set_default(&self, id: Uuid) -> Result<()> {
        VaultRegistry::set_default(self, id).await
    }

    async fn list_entries(
        &self,
        vault_id: Uuid,
        query: Option<&str>,
        identity: &AgeIdentity,
    ) -> Result<Vec<Entry>> {
        self.storage.list_entries(vault_id, query, identity).await
    }

    async fn get_entry(&self, id: Uuid, identity: &AgeIdentity) -> Result<Option<Entry>> {
        self.storage.get_entry(id, identity).await
    }

    async fn save_entry(&self, entry: &Entry, identity: &AgeIdentity) -> Result<()> {
        let vault = self
            .storage
            .list_vaults()
            .await?
            .into_iter()
            .find(|vault| vault.id == entry.vault_id)
            .ok_or(DbError::VaultNotFound(entry.vault_id))?;
        self.vault_service_for(&vault, identity.clone())
            .save_entry(entry)
            .await
    }

    async fn delete_entry(&self, id: Uuid) -> Result<()> {
        self.storage.delete_entry(id).await
    }
}

// endregion: --- Ports

// region:    --- Tests
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/vault/registry.rs"]
mod registry_test;
// endregion: --- Tests
