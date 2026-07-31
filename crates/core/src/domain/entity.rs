//! Entry and field types

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A vault entry containing fields with granular encryption control
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    Token,
    Otp,
    SshKey,
    Note,
}

/// A field in an entry with typed value and encryption control
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    Token,
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
