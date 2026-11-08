use crate::views::entry_form::{EntryForm, FormAction, FormFieldType};
use bogita_core::domain::{FieldType, FieldValue};
use ratatui::crossterm::event::KeyCode;

use super::support::*;

// ── domain mapping ────────────────────────────────────────────────────────────

#[test]
fn token_type_produces_hidden_field_value() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    for c in "password".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → value
    for c in "s3cr3t".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
                                   // navigate to Token (index 2): j=Username, j=Token
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Enter); // confirm selection
    form.handle_key(KeyCode::Tab); // → wrap back to name
    let FormAction::Confirm(entry) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };
    assert_eq!(entry.fields[0].key, "password");
    assert!(matches!(entry.fields[0].value, FieldValue::Hidden(_)));
    assert_eq!(entry.fields[0].field_type, FieldType::Token);
}

#[test]
fn text_type_produces_text_field_value_with_custom_field_type() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    for c in "url".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → value
    for c in "https://github.com".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    // leave type as Text (default — no type slot interaction)
    let FormAction::Confirm(entry) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };
    assert!(matches!(&entry.fields[0].value, FieldValue::Text(v) if v == "https://github.com"));
    assert_eq!(
        entry.fields[0].field_type,
        FieldType::Custom("url".to_string())
    );
}

#[test]
fn username_type_produces_text_field_value_with_username_field_type() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    for c in "alice".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // → 1=Username
    form.handle_key(KeyCode::Enter); // confirm selection
    form.handle_key(KeyCode::Tab); // → name
    let FormAction::Confirm(entry) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };
    assert!(matches!(&entry.fields[0].value, FieldValue::Text(v) if v == "alice"));
    assert_eq!(entry.fields[0].field_type, FieldType::Username);
}

#[test]
fn sshkey_type_produces_hidden_field_value_with_sshprivatekey_field_type() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    for c in "-----BEGIN".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Char('j')); // Totp
    form.handle_key(KeyCode::Char('j')); // SshKey
    form.handle_key(KeyCode::Enter); // confirm selection
    form.handle_key(KeyCode::Tab); // → name
    let FormAction::Confirm(entry) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };
    assert!(matches!(&entry.fields[0].value, FieldValue::Hidden(_)));
    assert_eq!(entry.fields[0].field_type, FieldType::SshPrivateKey);
}

// ── Esc cancels ───────────────────────────────────────────────────────────────

#[test]
fn esc_always_cancels_form() {
    let mut form = add_form_named("GitHub");
    // Form has a name, so it's dirty — Esc returns ConfirmDiscard.
    assert_eq!(form.handle_key(KeyCode::Esc), FormAction::ConfirmDiscard);
}

#[test]
fn esc_cancels_from_type_slot() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot
                                   // Form has a name and a field, so it's dirty — Esc returns ConfirmDiscard.
    assert_eq!(form.handle_key(KeyCode::Esc), FormAction::ConfirmDiscard);
}

// ── edit mode pre-fills ───────────────────────────────────────────────────────

#[test]
fn edit_form_pre_fills_name_and_fields() {
    use bogita_core::domain::{Entry, EntryType, Field};
    use chrono::Utc;
    use uuid::Uuid;

    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: "MyEntry".to_string(),
        entry_type: EntryType::Token,
        fields: vec![Field {
            id: Uuid::new_v4(),
            key: "username".to_string(),
            value: FieldValue::Text("alice".to_string()),
            field_type: FieldType::Username,
            encrypted: false,
            idx: 0,
        }],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    };
    let form = EntryForm::new_edit(&entry);
    assert_eq!(form.name(), "MyEntry");
    assert_eq!(form.field_count(), 1);
}

#[test]
fn edit_form_maps_token_field_type_to_form_token_type() {
    use bogita_core::domain::{Entry, EntryType, Field};
    use chrono::Utc;
    use uuid::Uuid;

    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: "MyEntry".to_string(),
        entry_type: EntryType::Token,
        fields: vec![Field {
            id: Uuid::new_v4(),
            key: "password".to_string(),
            value: FieldValue::Hidden("s3cr3t".to_string()),
            field_type: FieldType::Token,
            encrypted: true,
            idx: 0,
        }],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    };
    // Confirm should round-trip to Hidden value
    let FormAction::Confirm(e) = EntryForm::new_edit(&entry).handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };
    assert!(matches!(e.fields[0].value, FieldValue::Hidden(_)));
    assert_eq!(e.fields[0].field_type, FieldType::Token);
}

#[test]
fn edit_form_maps_sshprivatekey_to_sshkey_form_type() {
    use bogita_core::domain::{Entry, EntryType, Field};
    use chrono::Utc;
    use uuid::Uuid;

    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: "MyKey".to_string(),
        entry_type: EntryType::Token,
        fields: vec![Field {
            id: Uuid::new_v4(),
            key: "private_key".to_string(),
            value: FieldValue::Hidden("-----BEGIN".to_string()),
            field_type: FieldType::SshPrivateKey,
            encrypted: true,
            idx: 0,
        }],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    };
    let FormAction::Confirm(e) = EntryForm::new_edit(&entry).handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };
    assert!(matches!(e.fields[0].value, FieldValue::Hidden(_)));
    assert_eq!(e.fields[0].field_type, FieldType::SshPrivateKey);
}

// ── focused_slot_label ────────────────────────────────────────────────────────

#[test]
fn slot_label_on_name_slot() {
    let form = add_form();
    assert_eq!(form.focused_slot_label(), "name");
}

#[test]
fn slot_label_on_key_slot() {
    let mut form = add_form();
    form.handle_key(KeyCode::Char('+')); // adds field, focus → key slot of field 0
    assert_eq!(form.focused_slot_label(), "key");
}

#[test]
fn slot_label_on_value_slot_plain() {
    let mut form = add_form();
    form.handle_key(KeyCode::Char('+')); // focus → key slot
    form.handle_key(KeyCode::Tab); // focus → value slot (type=Text, not obscured)
    assert_eq!(form.focused_slot_label(), "value");
}

#[test]
fn slot_label_on_value_slot_obscured() {
    let mut form = add_form();
    form.handle_key(KeyCode::Char('+')); // focus → key slot
    form.handle_key(KeyCode::Tab); // focus → value slot
    form.handle_key(KeyCode::Tab); // focus → type slot
                                   // select Token (index 2): j=Username, j=Token
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Enter); // confirm selection
    form.handle_key(KeyCode::BackTab); // back to value slot
    assert_eq!(form.focused_slot_label(), "token");
}

#[test]
fn slot_label_on_type_slot() {
    let mut form = add_form();
    form.handle_key(KeyCode::Char('+')); // focus → key slot
    form.handle_key(KeyCode::Tab); // value
    form.handle_key(KeyCode::Tab); // type slot
    assert_eq!(form.focused_slot_label(), "type");
}

// ── field_badge ───────────────────────────────────────────────────────────────

#[test]
fn field_badge_for_plain_field_is_text() {
    let mut form = add_form();
    form.handle_key(KeyCode::Char('+')); // adds field, type=Text
    assert_eq!(form.field_badge(0), "text");
}

#[test]
fn field_badge_for_token_field_is_token() {
    let mut form = add_form();
    form.handle_key(KeyCode::Char('+')); // adds field, focus → key
    form.handle_key(KeyCode::Tab); // value
    form.handle_key(KeyCode::Tab); // type slot
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Enter); // confirm selection
    assert_eq!(form.field_badge(0), "token");
}

#[test]
fn field_badge_out_of_bounds_returns_empty() {
    let form = add_form();
    assert_eq!(form.field_badge(99), "");
}

// ── FormFieldType variant list ────────────────────────────────────────────────

#[test]
fn form_field_type_is_obscured_for_token_totp_sshkey() {
    assert!(!FormFieldType::Text.is_obscured());
    assert!(!FormFieldType::Username.is_obscured());
    assert!(FormFieldType::Token.is_obscured());
    assert!(FormFieldType::Totp.is_obscured());
    assert!(FormFieldType::SshKey.is_obscured());
}

#[test]
fn form_field_type_is_encrypted_for_token_totp_sshkey() {
    assert!(!FormFieldType::Text.is_encrypted());
    assert!(!FormFieldType::Username.is_encrypted());
    assert!(FormFieldType::Token.is_encrypted());
    assert!(FormFieldType::Totp.is_encrypted());
    assert!(FormFieldType::SshKey.is_encrypted());
}
