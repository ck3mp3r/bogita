//! VaultRegistry — manages multiple vaults, constructs VaultService per vault.

use crate::domain::{AgeIdentity, Vault};
use crate::error::Result;
use crate::ports::{Crypto, EntryStore, VaultStore};
use crate::vault::service::VaultService;
use uuid::Uuid;

/// Registry that manages vault metadata and constructs a `VaultService` per vault.
///
/// The registry owns the shared storage and crypto adapters. A `VaultService` is
/// constructed on demand for a specific vault, bound to that vault's recipients
/// and the caller's identity.
#[derive(Clone)]
pub struct VaultRegistry<S, C>
where
    S: VaultStore,
    C: Crypto,
{
    storage: S,
    crypto: C,
}

impl<S, C> VaultRegistry<S, C>
where
    S: VaultStore,
    C: Crypto + Clone,
{
    pub fn new(storage: S, crypto: C) -> Self {
        Self { storage, crypto }
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

    /// Construct a `VaultService` scoped to the given vault and identity.
    ///
    /// The service uses the vault's recipients for encryption and the supplied
    /// identity for decryption. The registry's storage and crypto are borrowed
    /// by reference so no cloning of heavy state is required.
    pub fn vault_service_for(&self, vault: &Vault, identity: AgeIdentity) -> VaultService<&S, C>
    where
        S: VaultStore + EntryStore,
    {
        VaultService::new(
            &self.storage,
            self.crypto.clone(),
            vault.recipients.clone(),
            identity,
        )
    }
}
