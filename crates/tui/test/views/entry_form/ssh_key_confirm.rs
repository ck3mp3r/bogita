//! Confirm-time validation for SshKey rows (`views/entry_form/confirm.rs`).
//!
//! The form is the door free-text SSH keys enter through: an unparsable value
//! must refuse at confirm instead of reaching storage, where the daemon can
//! only skip it.

use crate::views::entry_form::{EntryForm, FormAction};
use bogita_core::domain::{Entry, EntryType, Field, FieldType, FieldValue, SshKeyMaterial};
use bogita_core::test_support::AGE_ENCRYPTED_FILE;
use chrono::Utc;
use ratatui::crossterm::event::KeyCode;
use uuid::Uuid;

// ── fixtures ──────────────────────────────────────────────────────────────

fn ssh_entry_with_private_value(pem: &str) -> Entry {
    Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: "MyKey".to_string(),
        entry_type: EntryType::SshKey,
        fields: vec![Field {
            id: Uuid::new_v4(),
            key: "private_key".to_string(),
            value: FieldValue::Hidden(pem.to_string()),
            field_type: FieldType::SshPrivateKey,
            encrypted: true,
            idx: 0,
        }],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    }
}

// ── confirm ───────────────────────────────────────────────────────────────

#[test]
fn test_entry_form_confirm_refuses_unparsable_private_key_row() {
    // -- Setup & Fixtures
    let entry = ssh_entry_with_private_value(AGE_ENCRYPTED_FILE);
    let mut form = EntryForm::new_edit(&entry);

    // -- Exec
    let action = form.handle_key(KeyCode::Enter);

    // -- Check
    let FormAction::ValidationError(message) = action else {
        panic!("expected ValidationError, got {action:?}");
    };
    assert!(
        message.contains("private key is not a valid OpenSSH private key"),
        "the message names the defect: {message}"
    );
    assert!(
        message.contains("Field 1"),
        "the message names the row: {message}"
    );
}

#[test]
fn test_entry_form_confirm_accepts_keygen_authored_private_key_row() {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("form-door@test").expect("keygen an Ed25519 key");
    let entry = ssh_entry_with_private_value(material.private_pem());
    let mut form = EntryForm::new_edit(&entry);

    // -- Exec
    let action = form.handle_key(KeyCode::Enter);

    // -- Check
    let FormAction::Confirm(confirmed) = action else {
        panic!("expected Confirm, got {action:?}");
    };
    assert_eq!(confirmed.fields[0].field_type, FieldType::SshPrivateKey);
    assert!(matches!(&confirmed.fields[0].value, FieldValue::Hidden(_)));
}
