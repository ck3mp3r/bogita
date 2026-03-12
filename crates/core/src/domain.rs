//! Domain models and types

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// Re-export types from dependencies for convenience
pub use age::x25519;
pub use secrecy::{ExposeSecret, SecretString};

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
// Entry and Entry Types
// ============================================================================

/// A vault entry containing encrypted credential data
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    /// Unique identifier (auto-increment from SQLite)
    pub id: i64,

    /// Vault this entry belongs to
    pub vault_id: i64,

    /// Entry name/title (unique per vault)
    pub name: String,

    /// Type of entry
    pub entry_type: EntryType,

    /// Creation timestamp (Unix epoch seconds)
    pub created_at: i64,

    /// Last modification timestamp
    pub modified_at: i64,

    /// age-encrypted payload (JSON serialized EntryData)
    pub encrypted_data: Vec<u8>,

    /// Cleartext metadata for search/filtering
    pub metadata: EntryMetadata,
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

/// Cleartext metadata (NOT encrypted)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EntryMetadata {
    /// URL/website (e.g., "https://github.com")
    pub url: Option<String>,

    /// Username/email
    pub username: Option<String>,

    /// Plaintext notes
    pub notes: Option<String>,

    /// Favorite flag for quick access
    pub favorite: bool,
}

// ============================================================================
// Encrypted Entry Data
// ============================================================================

/// Decrypted entry data (enum for type-specific fields)
/// This is what gets JSON-serialized and then age-encrypted
/// Note: Secret<T> already handles zeroization
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EntryData {
    Password(PasswordData),
    Otp(OtpData),
    SshKey(SshKeyData),
    Note(NoteData),
}

/// Password entry data
/// Note: Passwords should be zeroized in memory when used in the application layer
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PasswordData {
    /// The actual password
    pub password: String,

    /// Optional password history (previous passwords)
    pub history: Vec<PasswordHistoryEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PasswordHistoryEntry {
    pub password: String, // Old passwords, less sensitive
    pub changed_at: i64,  // When it was changed
}

/// OTP/TOTP entry data
/// Note: Secrets should be zeroized in memory when used in the application layer
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OtpData {
    /// Base32-encoded secret key
    pub secret: String,

    /// Algorithm (default: SHA1 for compatibility)
    #[serde(default = "default_otp_algorithm")]
    pub algorithm: OtpAlgorithm,

    /// Number of digits (6 or 8, default: 6)
    #[serde(default = "default_otp_digits")]
    pub digits: u8,

    /// Period in seconds (default: 30)
    #[serde(default = "default_otp_period")]
    pub period: u64,

    /// Issuer (e.g., "GitHub")
    pub issuer: Option<String>,

    /// Account name (e.g., "user@example.com")
    pub account: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum OtpAlgorithm {
    SHA1,
    SHA256,
    SHA512,
}

pub fn default_otp_algorithm() -> OtpAlgorithm {
    OtpAlgorithm::SHA1
}

pub fn default_otp_digits() -> u8 {
    6
}

pub fn default_otp_period() -> u64 {
    30
}

/// SSH key entry data
/// Note: Private keys should be zeroized in memory when used in the application layer
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SshKeyData {
    /// Private key (OpenSSH or PEM format)
    pub private_key: String,

    /// Public key (derived or stored)
    pub public_key: String,

    /// Key type (e.g., "ssh-ed25519", "ssh-rsa")
    pub key_type: String,

    /// Comment/label
    pub comment: Option<String>,
}

/// Note entry data
/// Note: Content should be zeroized in memory when used in the application layer
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoteData {
    /// Secure note content
    pub content: String,
}

// ============================================================================
// Vault Metadata
// ============================================================================

/// Vault configuration and metadata
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vault {
    /// Unique identifier
    pub id: i64,

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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AwsConfig {
    /// AWS region (e.g., "us-east-1")
    pub region: String,

    /// Secret name prefix (e.g., "team-backend/")
    pub prefix: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GcpConfig {
    /// GCP project ID
    pub project_id: String,

    /// Secret name prefix
    pub prefix: String,
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
    pub entry_id: i64,

    /// Vault ID
    pub vault_id: i64,

    /// Type of operation
    pub operation: Operation,

    /// When the change occurred
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Encrypted entry data (full entry after change)
    pub encrypted_data: Vec<u8>,

    /// Entry metadata (cleartext)
    pub metadata: EntryMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Insert,
    Update,
    Delete,
}

/// Represents a sync conflict between local and remote
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conflict {
    /// Entry ID with conflict
    pub entry_id: i64,

    /// Local operation
    pub local_op: Operation,

    /// Remote operation
    pub remote_op: Operation,

    /// Local timestamp
    pub local_timestamp: chrono::DateTime<chrono::Utc>,

    /// Remote timestamp
    pub remote_timestamp: chrono::DateTime<chrono::Utc>,

    /// Local encrypted data
    pub local_data: Vec<u8>,

    /// Remote encrypted data
    pub remote_data: Vec<u8>,
}

/// Result of a push operation
#[derive(Clone, Debug)]
pub struct PushResult {
    /// Number of changes pushed
    pub changes_pushed: usize,

    /// Any conflicts encountered
    pub conflicts: Vec<Conflict>,
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
