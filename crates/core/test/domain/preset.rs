//! Tests for FieldPreset
//!
//! TDD: RED → GREEN → REFACTOR

use crate::domain::preset::FieldPreset;
use crate::domain::{EntryType, FieldType, FieldValue};
use uuid::Uuid;

fn vault_id() -> Uuid {
    Uuid::new_v4()
}

// ── Login ────────────────────────────────────────────────────────────────────

#[test]
fn test_login_preset_entry_type() {
    let entry = FieldPreset::Login.build("GitHub", vault_id());
    assert_eq!(entry.entry_type, EntryType::Token);
}

#[test]
fn test_login_preset_name() {
    let entry = FieldPreset::Login.build("GitHub", vault_id());
    assert_eq!(entry.name, "GitHub");
}

#[test]
fn test_login_preset_has_username_field() {
    let entry = FieldPreset::Login.build("GitHub", vault_id());
    let field = entry
        .fields
        .iter()
        .find(|f| f.field_type == FieldType::Username)
        .expect("should have username field");
    assert_eq!(field.key, "username");
    assert!(!field.encrypted, "username must not be encrypted");
    assert!(matches!(field.value, FieldValue::Text(_)));
}

#[test]
fn test_login_preset_has_password_field() {
    let entry = FieldPreset::Login.build("GitHub", vault_id());
    let field = entry
        .fields
        .iter()
        .find(|f| f.field_type == FieldType::Token)
        .expect("should have password field");
    assert_eq!(field.key, "password");
    assert!(field.encrypted, "password must be encrypted");
    assert!(matches!(field.value, FieldValue::Hidden(_)));
}

#[test]
fn test_login_preset_has_url_field() {
    let entry = FieldPreset::Login.build("GitHub", vault_id());
    let field = entry
        .fields
        .iter()
        .find(|f| f.field_type == FieldType::Url)
        .expect("should have url field");
    assert_eq!(field.key, "url");
    assert!(!field.encrypted, "url must not be encrypted");
    assert!(matches!(field.value, FieldValue::Url(_)));
}

#[test]
fn test_login_preset_field_count() {
    let entry = FieldPreset::Login.build("GitHub", vault_id());
    assert_eq!(entry.fields.len(), 3);
}

#[test]
fn test_login_preset_fields_have_unique_ids() {
    let entry = FieldPreset::Login.build("GitHub", vault_id());
    let ids: Vec<_> = entry.fields.iter().map(|f| f.id).collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len(), "field ids must be unique");
}

#[test]
fn test_login_preset_idx_order() {
    let entry = FieldPreset::Login.build("GitHub", vault_id());
    let idxs: Vec<i32> = entry.fields.iter().map(|f| f.idx).collect();
    assert_eq!(idxs, vec![0, 1, 2]);
}

// ── SshKey ───────────────────────────────────────────────────────────────────

#[test]
fn test_sshkey_preset_entry_type() {
    let entry = FieldPreset::SshKey.build("Deploy Key", vault_id());
    assert_eq!(entry.entry_type, EntryType::SshKey);
}

#[test]
fn test_sshkey_preset_has_private_key_field_encrypted() {
    let entry = FieldPreset::SshKey.build("Deploy Key", vault_id());
    let field = entry
        .fields
        .iter()
        .find(|f| f.field_type == FieldType::SshPrivateKey)
        .expect("should have private key field");
    assert!(field.encrypted, "private key must be encrypted");
    assert!(matches!(field.value, FieldValue::Hidden(_)));
}

#[test]
fn test_sshkey_preset_has_public_key_field_plaintext() {
    let entry = FieldPreset::SshKey.build("Deploy Key", vault_id());
    let field = entry
        .fields
        .iter()
        .find(|f| f.field_type == FieldType::SshPublicKey)
        .expect("should have public key field");
    assert!(!field.encrypted, "public key must not be encrypted");
    assert!(matches!(field.value, FieldValue::Text(_)));
}

#[test]
fn test_sshkey_preset_has_key_type_and_comment() {
    let entry = FieldPreset::SshKey.build("Deploy Key", vault_id());
    assert!(entry
        .fields
        .iter()
        .any(|f| f.field_type == FieldType::SshKeyType));
    assert!(entry
        .fields
        .iter()
        .any(|f| f.field_type == FieldType::SshComment));
}

#[test]
fn test_sshkey_preset_field_count() {
    let entry = FieldPreset::SshKey.build("Deploy Key", vault_id());
    assert_eq!(entry.fields.len(), 4);
}

// ── Note ─────────────────────────────────────────────────────────────────────

#[test]
fn test_note_preset_entry_type() {
    let entry = FieldPreset::Note.build("My Note", vault_id());
    assert_eq!(entry.entry_type, EntryType::Note);
}

#[test]
fn test_note_preset_has_notes_field() {
    let entry = FieldPreset::Note.build("My Note", vault_id());
    let field = entry
        .fields
        .iter()
        .find(|f| f.field_type == FieldType::Notes)
        .expect("should have notes field");
    assert_eq!(field.key, "notes");
    assert!(!field.encrypted, "note body is not encrypted by default");
    assert!(matches!(field.value, FieldValue::Text(_)));
}

#[test]
fn test_note_preset_field_count() {
    let entry = FieldPreset::Note.build("My Note", vault_id());
    assert_eq!(entry.fields.len(), 1);
}

// ── General ──────────────────────────────────────────────────────────────────

#[test]
fn test_preset_vault_id_is_set() {
    let vid = vault_id();
    let entry = FieldPreset::Login.build("Test", vid);
    assert_eq!(entry.vault_id, vid);
}

#[test]
fn test_preset_entry_has_non_nil_id() {
    let entry = FieldPreset::Login.build("Test", vault_id());
    assert_ne!(entry.id, Uuid::nil());
}

#[test]
fn test_preset_timestamps_are_positive() {
    let entry = FieldPreset::Login.build("Test", vault_id());
    assert!(entry.created_at > 0);
    assert!(entry.modified_at > 0);
}
