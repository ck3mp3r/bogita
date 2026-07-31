//! Vault metadata types

use crate::domain::key::AgeRecipient;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Vault configuration and metadata
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vault {
    /// Unique identifier (UUID v4 for sync-friendly IDs)
    pub id: Uuid,

    /// Vault name (unique)
    pub name: String,

    /// Is this the default vault?
    pub is_default: bool,

    /// Creation timestamp
    pub created_at: i64,

    /// Backend configuration
    pub backend: VaultBackend,

    /// age recipients (public keys for encryption)
    pub recipients: Vec<AgeRecipient>,

    /// Auto-lock timeout (seconds, None = never)
    pub lock_timeout: Option<u64>,

    /// Auto-sync on changes (Git only)
    pub auto_sync: bool,
}

/// Backend configuration (enum for different storage types)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum VaultBackend {
    Git(GitConfig),
    Aws(AwsConfig),
    Gcp(GcpConfig),
    Sqlite(SqliteConfig),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitConfig {
    /// Local git repository path
    pub path: String,

    /// Remote URL (e.g., "git@github.com:user/vault.git")
    pub remote: String,
    // Note: Git always uses age encryption (untrusted storage)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AwsConfig {
    /// AWS region (e.g., "us-east-1")
    pub region: String,

    /// Secret name prefix (e.g., "team-backend/")
    pub prefix: String,

    /// Enable double encryption (age + AWS KMS)
    ///
    /// When false (default): Store plaintext, AWS encrypts with KMS
    /// When true: Age-encrypt before storing, AWS encrypts the encrypted blob
    ///
    /// Use true for zero-trust scenarios where you don't trust the cloud provider
    #[serde(default)]
    pub double_encrypt: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GcpConfig {
    /// GCP project ID
    pub project_id: String,

    /// Secret name prefix
    pub prefix: String,

    /// Enable double encryption (age + GCP envelope encryption)
    ///
    /// When false (default): Store plaintext, GCP encrypts automatically
    /// When true: Age-encrypt before storing, GCP encrypts the encrypted blob
    ///
    /// Use true for zero-trust scenarios where you don't trust the cloud provider
    #[serde(default)]
    pub double_encrypt: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SqliteConfig {
    /// Database file path
    pub path: String,
}
