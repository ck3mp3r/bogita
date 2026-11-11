//! Where the daemon gets its keys when a frontend unlocks the vault.
//!
//! The daemon holds the vault registry, not the raw storage adapter, so a ring
//! rebuild reads through the same ports the TUI uses. Static dispatch only.

use crate::domain::{AgeIdentity, Entry};
use crate::error::Result;
use crate::ports::{EntryStore, VaultStore};
use crate::vault::registry::VaultRegistry;
use async_trait::async_trait;

// region:    --- Source

/// Where the ring gets its keys when the vault unlocks.
#[async_trait]
pub trait UnlockSource: Send + Sync {
    /// Every entry of every vault, decrypted through `identity`.
    async fn entries(&self, identity: &AgeIdentity) -> Result<Vec<Entry>>;
}

#[async_trait]
impl<S> UnlockSource for VaultRegistry<S>
where
    S: VaultStore + EntryStore,
{
    async fn entries(&self, identity: &AgeIdentity) -> Result<Vec<Entry>> {
        let mut entries = Vec::new();
        for vault in self.list_vaults().await? {
            let service = self.vault_service_for(&vault, identity.clone());
            entries.extend(service.list_entries(vault.id, None).await?);
        }
        Ok(entries)
    }
}

// endregion: --- Source
