//! VaultService — application layer coordinating storage and crypto adapters.

use crate::domain::{AgeIdentity, AgeRecipient, Entry};
use crate::error::Result;
use crate::ports::{Crypto, EntryStore};
use uuid::Uuid;

/// Application service that coordinates storage and crypto via static dispatch.
pub struct VaultService<S, C>
where
    S: EntryStore,
    C: Crypto,
{
    storage: S,
    #[allow(dead_code)]
    crypto: C,
    recipients: Vec<AgeRecipient>,
    identity: AgeIdentity,
}

impl<S, C> VaultService<S, C>
where
    S: EntryStore,
    C: Crypto,
{
    pub fn new(
        storage: S,
        crypto: C,
        recipients: Vec<AgeRecipient>,
        identity: AgeIdentity,
    ) -> Self {
        Self {
            storage,
            crypto,
            recipients,
            identity,
        }
    }

    pub async fn add_entry(&self, entry: &Entry) -> Result<()> {
        self.storage.save_entry(entry, &self.recipients).await
    }

    pub async fn update_entry(&self, entry: &Entry) -> Result<()> {
        self.storage.save_entry(entry, &self.recipients).await
    }

    pub async fn get_entry(&self, id: Uuid) -> Result<Option<Entry>> {
        self.storage.get_entry(id, &self.identity).await
    }

    pub async fn list_entries(&self, vault_id: Uuid, query: Option<&str>) -> Result<Vec<Entry>> {
        self.storage
            .list_entries(vault_id, query, &self.identity)
            .await
    }

    pub async fn delete_entry(&self, id: Uuid) -> Result<()> {
        self.storage.delete_entry(id).await
    }
}

// Test helper — delegates to storage's seed method
#[cfg(test)]
impl<C> VaultService<crate::storage::sqlite::SqliteStorage<C>, C>
where
    C: Crypto + Send + Sync,
{
    pub async fn seed_vault_for_test(&self, vault_id: Uuid) -> Result<()> {
        self.storage.seed_vault_for_test(vault_id).await
    }
}
