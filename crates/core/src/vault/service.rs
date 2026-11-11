//! VaultEntryService — application layer coordinating entry operations over the storage adapter.

use crate::domain::{AgeIdentity, AgeRecipient, Entry};
use crate::error::Result;
use crate::ports::EntryStore;
use uuid::Uuid;

/// Application service that coordinates entry storage via static dispatch.
///
/// Encrypted fields are written through the storage adapter with this vault's
/// recipients; reads pass the caller's identity to the same adapter.
pub struct VaultEntryService<S>
where
    S: EntryStore,
{
    storage: S,
    recipients: Vec<AgeRecipient>,
    identity: AgeIdentity,
}

impl<S> VaultEntryService<S>
where
    S: EntryStore,
{
    pub fn new(storage: S, recipients: Vec<AgeRecipient>, identity: AgeIdentity) -> Self {
        Self {
            storage,
            recipients,
            identity,
        }
    }

    /// Persist an entry, encrypting its fields with this vault's recipients.
    pub async fn save_entry(&self, entry: &Entry) -> Result<()> {
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

// region:    --- Tests
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/vault/service.rs"]
mod service_test;
// endregion: --- Tests
