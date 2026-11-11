use crate::views::entry_form::{EntryForm, FormAction};
use bogita_core::domain::{Entry, EntryType, Field, FieldType, FieldValue, AGENT_SERVABLE_KEY};
use chrono::Utc;
use ratatui::crossterm::event::KeyCode;
use uuid::Uuid;

use super::support::*;

// ── boolean field type ────────────────────────────────────────────────────────

#[test]
fn test_entry_form_new_row_boolean_type_authors_boolean_value() {
    // -- Setup & Fixtures
    let mut form = add_form_named("GitHub");
    add_field(&mut form); // new field, focus → key slot
    for c in "enabled".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    for _ in 0..7 {
        form.handle_key(KeyCode::Char('j')); // → 7 = boolean
    }
    form.handle_key(KeyCode::Enter); // apply the selection

    // -- Exec
    form.handle_key(KeyCode::Tab); // → value slot
    form.handle_key(KeyCode::Char(' ')); // Space is inert on the boolean row
    assert_eq!(
        form.fields[0].value_state.text(),
        "",
        "Space must not seed the boolean cell"
    );
    flip_boolean(&mut form); // seeds the sense as true
    let first = match form.confirm() {
        FormAction::Confirm(entry) => entry,
        FormAction::ValidationError(message) => panic!("unexpected validation error: {message}"),
        _ => panic!("expected Confirm"),
    };
    flip_boolean(&mut form); // true → false
    let FormAction::Confirm(entry) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };

    // -- Check
    assert_eq!(form.field_badge(0), "boolean");
    assert!(!form.focused_field_is_obscured());
    assert_eq!(first.fields[0].key, "enabled");
    assert_eq!(first.fields[0].field_type, FieldType::Boolean);
    assert_eq!(first.fields[0].value, FieldValue::Boolean(true));
    assert!(!first.fields[0].encrypted);
    assert_eq!(entry.fields[0].key, "enabled");
    assert_eq!(entry.fields[0].field_type, FieldType::Boolean);
    assert_eq!(entry.fields[0].value, FieldValue::Boolean(false));
    assert!(!entry.fields[0].encrypted);
}

#[test]
fn test_entry_form_stored_boolean_row_round_trips_boolean() {
    // -- Setup & Fixtures
    let now = Utc::now().timestamp();
    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: "Feature flags".to_string(),
        entry_type: EntryType::Token,
        fields: vec![Field {
            id: Uuid::new_v4(),
            key: "enabled".to_string(),
            value: FieldValue::Boolean(true),
            field_type: FieldType::Boolean,
            encrypted: false,
            idx: 0,
        }],
        created_at: now,
        modified_at: now,
    };
    let mut form = EntryForm::new_edit(&entry);

    // -- Exec
    let FormAction::Confirm(confirmed) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };

    // -- Check
    assert_eq!(form.field_badge(0), "boolean");
    assert_eq!(confirmed.fields[0].key, "enabled");
    assert_eq!(confirmed.fields[0].field_type, FieldType::Boolean);
    assert_eq!(confirmed.fields[0].value, FieldValue::Boolean(true));
    assert!(!confirmed.fields[0].encrypted);
}

#[test]
fn test_entry_form_legacy_custom_servable_row_loads_as_boolean() {
    // -- Setup & Fixtures
    let now = Utc::now().timestamp();
    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: "Deploy key".to_string(),
        entry_type: EntryType::Token,
        fields: vec![Field {
            id: Uuid::new_v4(),
            key: AGENT_SERVABLE_KEY.to_string(),
            value: FieldValue::Boolean(true),
            field_type: FieldType::Custom(AGENT_SERVABLE_KEY.to_string()),
            encrypted: false,
            idx: 0,
        }],
        created_at: now,
        modified_at: now,
    };
    let mut form = EntryForm::new_edit(&entry);

    // -- Exec
    form.handle_key(KeyCode::Tab); // → key slot
    form.handle_key(KeyCode::Tab); // → type slot
    form.handle_key(KeyCode::Tab); // → value slot
    flip_boolean(&mut form); // true → false
    let FormAction::Confirm(confirmed) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };

    // -- Check
    assert_eq!(form.field_badge(0), "boolean");
    assert_eq!(confirmed.fields[0].key, AGENT_SERVABLE_KEY);
    assert_eq!(confirmed.fields[0].field_type, FieldType::Boolean);
    assert_eq!(confirmed.fields[0].value, FieldValue::Boolean(false));
    assert!(!confirmed.fields[0].encrypted);
}
