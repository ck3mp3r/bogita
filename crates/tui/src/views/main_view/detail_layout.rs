//! Field-inventory layout — which groups a pane owns, read from the entry's fields.
//!
//! The category IS the inventory. Nothing here reads the stored category column:
//! it is user-side unreliable (old saves wrote SSH entries as `token`) and no
//! runtime consumer — keyring, git-signing — trusts it either; every consumer keys
//! on the per-field truth, so the pane does the same. Rows group by field
//! semantics: field type first, canonical key name second. Every group renders
//! only when it has rows — the empty-group rule is global, never per-group.

use bogita_core::domain::{Entry, Field, FieldType, AGENT_SERVABLE_KEY};

// region:    --- Constants

/// The canonical field keys each category owns, in display order.
const TOKEN_KEYS: &[&str] = &[
    "username",
    "password",
    "url",
    "totp_secret",
    "totp_period",
    "totp_digits",
    "totp_issuer",
    "totp_account",
];
const SSH_KEYS: &[&str] = &["private_key", "public_key", "comment"];
const KEY_TYPE_KEY: &str = "key_type";
const NOTE_KEYS: &[&str] = &["notes"];
const OTP_KEYS: &[&str] = &["secret", "totp_secret", "totp_period", "totp_digits"];

// endregion: --- Constants

// region:    --- Types

/// One rendered group: its header, an optional badge, and its rows.
pub(super) struct LayoutSection<'a> {
    pub(super) title: &'static str,
    pub(super) badge: Option<String>,
    pub(super) rows: Vec<(usize, &'a Field)>,
}

/// The groups the entry's field inventory owns, in display order.
pub(super) struct CategoryLayout<'a> {
    pub(super) sections: Vec<LayoutSection<'a>>,
}

// endregion: --- Types

// region:    --- Compose

impl<'a> CategoryLayout<'a> {
    /// Read the layout off the entry's field inventory.
    pub(super) fn of(entry: &'a Entry) -> Self {
        let badge = key_type_badge(entry);
        // An entry that carries a TOTP seed outside the credential key set is an
        // OTP entry: its whole time-based family (seed, period, digits) is the
        // code-first OTP group, not credential rows.
        let otp_mode = entry.fields.iter().any(|field| {
            field.field_type == FieldType::TotpSecret && !TOKEN_KEYS.contains(&field.key.as_str())
        });
        let mut ssh: Vec<(usize, &Field)> = Vec::new();
        let mut credentials: Vec<(usize, &Field)> = Vec::new();
        let mut otp: Vec<(usize, &Field)> = Vec::new();
        let mut notes: Vec<(usize, &Field)> = Vec::new();
        let mut other: Vec<(usize, &Field)> = Vec::new();

        for (idx, field) in entry.fields.iter().enumerate() {
            let key = field.key.as_str();
            // Entry-level state: the pill lives in METADATA.
            if field.key == AGENT_SERVABLE_KEY {
                continue;
            }
            // The key-type row is the SSH badge, not a row of its own.
            if key == KEY_TYPE_KEY {
                continue;
            }
            if is_ssh_material(field, key) {
                ssh.push((idx, field));
            } else if NOTE_KEYS.contains(&key) {
                notes.push((idx, field));
            } else if OTP_KEYS.contains(&key) && (otp_mode || !TOKEN_KEYS.contains(&key)) {
                otp.push((idx, field));
            } else if is_credential(field, key) {
                credentials.push((idx, field));
            } else {
                other.push((idx, field));
            }
        }

        ssh.sort_by_key(|(_, field)| position_in(SSH_KEYS, field));
        credentials.sort_by_key(|(_, field)| position_in(TOKEN_KEYS, field));
        otp.sort_by_key(|(_, field)| position_in(OTP_KEYS, field));

        // Global skip-empty: a group exists only when it has rows.
        let mut sections = Vec::new();
        for (title, badge, rows) in [
            ("SSH", badge, ssh),
            ("CREDENTIALS", None, credentials),
            ("OTP", None, otp),
            ("NOTE", None, notes),
            ("OTHER FIELDS", None, other),
        ] {
            if !rows.is_empty() {
                sections.push(LayoutSection { title, badge, rows });
            }
        }

        Self { sections }
    }
}

/// `true` when the row is SSH key material, by type or by canonical key name.
fn is_ssh_material(field: &Field, key: &str) -> bool {
    matches!(
        field.field_type,
        FieldType::SshPrivateKey
            | FieldType::SshPublicKey
            | FieldType::SshKeyType
            | FieldType::SshComment
    ) || SSH_KEYS.contains(&key)
}

/// `true` when the row is a credential, by type or by canonical key name.
fn is_credential(field: &Field, key: &str) -> bool {
    matches!(
        field.field_type,
        FieldType::Username | FieldType::Token | FieldType::TotpSecret
    ) || TOKEN_KEYS.contains(&key)
}

/// Canonical display position for a key, `usize::MAX` when not canonical.
fn position_in(keys: &[&str], field: &Field) -> usize {
    keys.iter()
        .position(|key| *key == field.key)
        .unwrap_or(usize::MAX)
}

/// The key-type badge, read from the row whose KEY is `key_type` — the row's
/// value, never its type: real vaults store the row as a custom type.
fn key_type_badge(entry: &Entry) -> Option<String> {
    entry
        .fields
        .iter()
        .find(|field| field.key == KEY_TYPE_KEY)
        .map(|field| field.value.to_string().trim().to_uppercase())
        .filter(|badge| !badge.is_empty())
}

// endregion: --- Compose
