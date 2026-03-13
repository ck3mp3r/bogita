//! FieldPreset — starting field configurations for new entries.
//!
//! A preset produces a fully formed `Entry` with sensible defaults.
//! All fields are mutable after construction — the preset is a starting
//! point, not a constraint.

use crate::domain::{Entry, EntryType, Field, FieldType, FieldValue};
use uuid::Uuid;

/// A named starting configuration for a new entry.
pub enum FieldPreset {
    /// Username + Password + URL
    Login,
    /// Private key (encrypted) + Public key + Key type + Comment
    SshKey,
    /// Freeform text note
    Note,
}

impl FieldPreset {
    /// Build a new `Entry` with the preset's default fields.
    ///
    /// The returned entry has a fresh UUID, current timestamps, and
    /// empty (blank) values — ready for the caller to fill in.
    pub fn build(self, name: &str, vault_id: Uuid) -> Entry {
        let now = chrono::Utc::now().timestamp();
        let (entry_type, fields) = match self {
            FieldPreset::Login => (EntryType::Password, login_fields()),
            FieldPreset::SshKey => (EntryType::SshKey, sshkey_fields()),
            FieldPreset::Note => (EntryType::Note, note_fields()),
        };
        Entry {
            id: Uuid::new_v4(),
            vault_id,
            name: name.to_string(),
            entry_type,
            created_at: now,
            modified_at: now,
            fields,
        }
    }
}

fn login_fields() -> Vec<Field> {
    vec![
        Field {
            id: Uuid::new_v4(),
            key: "username".to_string(),
            value: FieldValue::Text(String::new()),
            field_type: FieldType::Username,
            encrypted: false,
            idx: 0,
        },
        Field {
            id: Uuid::new_v4(),
            key: "password".to_string(),
            value: FieldValue::Hidden(String::new()),
            field_type: FieldType::Password,
            encrypted: true,
            idx: 1,
        },
        Field {
            id: Uuid::new_v4(),
            key: "url".to_string(),
            value: FieldValue::Url(String::new()),
            field_type: FieldType::Url,
            encrypted: false,
            idx: 2,
        },
    ]
}

fn sshkey_fields() -> Vec<Field> {
    vec![
        Field {
            id: Uuid::new_v4(),
            key: "private_key".to_string(),
            value: FieldValue::Hidden(String::new()),
            field_type: FieldType::SshPrivateKey,
            encrypted: true,
            idx: 0,
        },
        Field {
            id: Uuid::new_v4(),
            key: "public_key".to_string(),
            value: FieldValue::Text(String::new()),
            field_type: FieldType::SshPublicKey,
            encrypted: false,
            idx: 1,
        },
        Field {
            id: Uuid::new_v4(),
            key: "key_type".to_string(),
            value: FieldValue::Text(String::new()),
            field_type: FieldType::SshKeyType,
            encrypted: false,
            idx: 2,
        },
        Field {
            id: Uuid::new_v4(),
            key: "comment".to_string(),
            value: FieldValue::Text(String::new()),
            field_type: FieldType::SshComment,
            encrypted: false,
            idx: 3,
        },
    ]
}

fn note_fields() -> Vec<Field> {
    vec![Field {
        id: Uuid::new_v4(),
        key: "notes".to_string(),
        value: FieldValue::Text(String::new()),
        field_type: FieldType::Notes,
        encrypted: false,
        idx: 0,
    }]
}
