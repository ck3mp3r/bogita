//! Sync port trait
//!
//! Defines the interface for syncing entries with remote backends.

use crate::domain::{Change, SyncMetadata};
use crate::error::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Sync port for pushing/pulling changes to/from remote backends
///
/// This trait defines the interface for sync adapters (Git, AWS, GCP).
/// All operations are async as they involve network I/O.
#[async_trait]
pub trait SyncBackend: Send + Sync {
    /// Push local changes to remote
    ///
    /// Pushes all changes since last_sync to the remote backend.
    /// Returns the number of changes successfully pushed.
    async fn push(&self, changes: &[Change]) -> Result<usize>;

    /// Pull remote changes to local
    ///
    /// Fetches all changes from remote since the given timestamp.
    /// Returns changes that need to be applied locally.
    async fn pull(&self, since: Option<DateTime<Utc>>) -> Result<Vec<Change>>;

    /// Get timestamp of last successful sync
    ///
    /// Returns None if never synced before.
    async fn last_sync(&self) -> Result<Option<DateTime<Utc>>>;

    /// Get metadata about the sync backend
    ///
    /// Returns information about the backend type, identifier, and capabilities.
    async fn metadata(&self) -> Result<SyncMetadata>;
}
