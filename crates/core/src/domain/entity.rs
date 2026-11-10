//! Entry and field types

use serde::{Deserialize, Serialize};
use std::fmt;
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

impl Entry {
    /// Compute the live TOTP code for `field_key` at `now_secs`.
    ///
    /// The secret comes from the `TotpSecret` field named `field_key`; the
    /// period and digit count come from the entry's sibling `TotpPeriod` and
    /// `TotpDigits` fields (defaults 30 and 6). Returns `None` when the field is
    /// missing or its secret cannot be decoded.
    ///
    /// An `otpauth://` URI is not a supported secret format and returns `None`.
    pub fn totp_code(&self, field_key: &str, now_secs: u64) -> Option<(String, u64)> {
        let secret_field = self
            .fields
            .iter()
            .find(|f| f.key == field_key && f.field_type == FieldType::TotpSecret)?;

        let secret_bytes =
            crate::service::otp::decode_secret(&secret_field.value.to_string()).ok()?;

        let period = self
            .fields
            .iter()
            .find(|f| f.field_type == FieldType::TotpPeriod)
            .and_then(|f| match &f.value {
                FieldValue::Number(n) => Some(*n as u64),
                FieldValue::Text(s) => s.parse().ok(),
                _ => None,
            })
            .unwrap_or(30);

        let digits = self
            .fields
            .iter()
            .find(|f| f.field_type == FieldType::TotpDigits)
            .and_then(|f| match &f.value {
                FieldValue::Number(n) => Some(*n as u32),
                FieldValue::Text(s) => s.parse().ok(),
                _ => None,
            })
            .unwrap_or(6);

        crate::service::otp::generate_totp(&secret_bytes, period, digits, now_secs).ok()
    }
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
    Boolean,

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

impl FieldType {
    /// Storage discriminator written to `entry_fields.field_type`.
    pub fn to_storage(&self) -> String {
        match self {
            Self::Username => "username".to_string(),
            Self::Token => "token".to_string(),
            Self::Url => "url".to_string(),
            Self::Notes => "notes".to_string(),
            Self::Tags => "tags".to_string(),
            Self::Favorite => "favorite".to_string(),
            Self::Boolean => "boolean".to_string(),
            Self::TotpSecret => "totp_secret".to_string(),
            Self::TotpAlgorithm => "totp_algorithm".to_string(),
            Self::TotpDigits => "totp_digits".to_string(),
            Self::TotpPeriod => "totp_period".to_string(),
            Self::TotpIssuer => "totp_issuer".to_string(),
            Self::TotpAccount => "totp_account".to_string(),
            Self::SshPrivateKey => "ssh_private_key".to_string(),
            Self::SshPublicKey => "ssh_public_key".to_string(),
            Self::SshKeyType => "ssh_key_type".to_string(),
            Self::SshComment => "ssh_comment".to_string(),
            Self::PasswordHistory => "password_history".to_string(),
            Self::Custom(name) => format!("custom:{name}"),
        }
    }

    /// Parse a storage discriminator back into a `FieldType`.
    ///
    /// Values with the `custom:` prefix and unknown discriminators both map to
    /// [`FieldType::Custom`], so rows written by newer builds stay readable.
    pub fn from_storage(value: &str) -> Self {
        match value.strip_prefix("custom:") {
            Some(name) => Self::Custom(name.to_string()),
            None => match value {
                "username" => Self::Username,
                "token" => Self::Token,
                "url" => Self::Url,
                "notes" => Self::Notes,
                "tags" => Self::Tags,
                "favorite" => Self::Favorite,
                "boolean" => Self::Boolean,
                "totp_secret" => Self::TotpSecret,
                "totp_algorithm" => Self::TotpAlgorithm,
                "totp_digits" => Self::TotpDigits,
                "totp_period" => Self::TotpPeriod,
                "totp_issuer" => Self::TotpIssuer,
                "totp_account" => Self::TotpAccount,
                "ssh_private_key" => Self::SshPrivateKey,
                "ssh_public_key" => Self::SshPublicKey,
                "ssh_key_type" => Self::SshKeyType,
                "ssh_comment" => Self::SshComment,
                "password_history" => Self::PasswordHistory,
                other => Self::Custom(other.to_string()),
            },
        }
    }
}

impl fmt::Display for FieldValue {
    /// Render the raw value. Masking is a presentation decision and stays with
    /// the caller; `Hidden` values print their plaintext here.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(value) | Self::Hidden(value) | Self::Url(value) | Self::Email(value) => {
                write!(f, "{value}")
            }
            Self::Boolean(value) => write!(f, "{value}"),
            Self::Number(value) => write!(f, "{value}"),
            Self::Date(ts) => write!(f, "{ts}"),
        }
    }
}
