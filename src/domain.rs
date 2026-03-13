//! Domain models and types

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// Re-export types from dependencies for convenience
pub use age::x25519;
pub use secrecy::{ExposeSecret, SecretString};
pub use uuid::Uuid;

/// Wrapper around age::x25519::Recipient (public key)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgeRecipient(x25519::Recipient);

impl AgeRecipient {
    pub fn inner(&self) -> &x25519::Recipient {
        &self.0
    }
}

impl FromStr for AgeRecipient {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<x25519::Recipient>()
            .map(AgeRecipient)
            .map_err(|e| format!("invalid age recipient: {}", e))
    }
}

impl fmt::Display for AgeRecipient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for AgeRecipient {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for AgeRecipient {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Wrapper around age::x25519::Identity (private key)
/// Note: x25519::Identity already handles zeroization internally
#[derive(Clone)]
pub struct AgeIdentity(x25519::Identity);

impl AgeIdentity {
    pub fn generate() -> Self {
        Self(x25519::Identity::generate())
    }

    pub fn to_recipient(&self) -> AgeRecipient {
        AgeRecipient(self.0.to_public())
    }

    pub fn to_secret_string(&self) -> SecretString {
        self.0.to_string()
    }

    pub fn inner(&self) -> &x25519::Identity {
        &self.0
    }
}

impl FromStr for AgeIdentity {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<x25519::Identity>()
            .map(AgeIdentity)
            .map_err(|e| format!("invalid age identity: {}", e))
    }
}

// ============================================================================
// Entry and Field Types
// ============================================================================

/// A vault entry containing fields with granular encryption control
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    /// Unique identifier (UUID v4 for sync-friendly IDs)
    pub id: Uuid,

    /// Vault this entry belongs to
    pub vault_id: Uuid,

    /// Entry name/title (unique per vault)
    pub name: String,

    /// Type of entry
    pub entry_type: EntryType,

    /// Creation timestamp (Unix epoch seconds)
    pub created_at: i64,

    /// Last modification timestamp
    pub modified_at: i64,

    /// Key-value fields with encryption control
    pub fields: Vec<Field>,
}

/// Entry type discriminator
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    Password,
    Otp,
    SshKey,
    Note,
}

/// A field in an entry with typed value and encryption control
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Field {
    /// Unique identifier for the field
    pub id: Uuid,

    /// Field key/name
    pub key: String,

    /// Field value (typed)
    pub value: FieldValue,

    /// Field type (semantic meaning)
    pub field_type: FieldType,

    /// Whether this field's value is encrypted in storage
    pub encrypted: bool,

    /// Display order index
    pub idx: i32,
}

/// Typed field values
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum FieldValue {
    /// Plain text value
    Text(String),

    /// Hidden text (passwords, secrets - visually obscured)
    Hidden(String),

    /// Boolean value
    Boolean(bool),

    /// Integer value
    Number(i64),

    /// URL value
    Url(String),

    /// Email address
    Email(String),

    /// Date/time (Unix timestamp)
    Date(i64),
}

/// Field type for semantic meaning
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    // Common fields
    Username,
    Password,
    Url,
    Notes,
    Tags,
    Favorite,

    // OTP/TOTP fields
    TotpSecret,
    TotpAlgorithm,
    TotpDigits,
    TotpPeriod,
    TotpIssuer,
    TotpAccount,

    // SSH key fields
    SshPrivateKey,
    SshPublicKey,
    SshKeyType,
    SshComment,

    // Password history (stored as JSON array in Text)
    PasswordHistory,

    // Custom user-defined field
    Custom(String),
}

// ============================================================================
// Vault Metadata
// ============================================================================

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

// ============================================================================
// Sync Types
// ============================================================================

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
