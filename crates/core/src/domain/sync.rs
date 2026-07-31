//! Sync types

use crate::domain::entity::Entry;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a change to an entry for sync purposes
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Change {
    /// Entry ID that changed
    pub entry_id: Uuid,

    /// Vault ID
    pub vault_id: Uuid,

    /// Type of operation
    pub operation: Operation,

    /// When the change occurred
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Full entry with all fields (some may be encrypted)
    pub entry: Entry,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Insert,
    Update,
    Delete,
}

/// Result of a push operation
#[derive(Clone, Debug)]
pub struct PushResult {
    /// Number of changes pushed
    pub changes_pushed: usize,
}

/// Metadata about a sync target
#[derive(Clone, Debug)]
pub struct SyncMetadata {
    /// Type of sync backend
    pub sync_type: SyncType,

    /// Human-readable identifier (URL, region, etc.)
    pub identifier: String,

    /// Does this backend support bidirectional sync?
    pub supports_bidirectional: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncType {
    Git,
    Aws,
    Gcp,
}
