use crate::views::entry_form::{EntryForm, FormAction};
use bogita_core::domain::{FieldType, FieldValue};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::*;

/// Press the Alt-s management chord.
fn alt_s(form: &mut EntryForm) -> FormAction {
    form.handle_key_with_modifiers(KeyCode::Char('s'), KeyModifiers::ALT)
}

#[test]
fn alt_s_from_type_slot_confirms_entry() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    for c in "password".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot (dropdown closed)

    let FormAction::Confirm(entry) = alt_s(&mut form) else {
        panic!("Alt-s on the type slot should confirm the form");
    };
    assert_eq!(entry.name, "GitHub");
    assert_eq!(entry.fields[0].key, "password");
}

#[test]
fn alt_s_with_open_dropdown_confirms_selection_and_saves() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    for c in "user".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // → 1 = username

    let FormAction::Confirm(entry) = alt_s(&mut form) else {
        panic!("Alt-s with the popup open should confirm the form");
    };
    assert_eq!(
        entry.fields[0].field_type,
        FieldType::Username,
        "Alt-s must apply the pending type selection before saving"
    );
}

#[test]
fn alt_s_from_value_slot_confirms_entry() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    for c in "password".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
    form.handle_key(KeyCode::Tab); // → value slot
    for c in "s3cret".chars() {
        form.handle_key(KeyCode::Char(c));
    }

    let FormAction::Confirm(entry) = alt_s(&mut form) else {
        panic!("Alt-s on the value slot should confirm the form");
    };
    assert!(matches!(&entry.fields[0].value, FieldValue::Text(v) if v == "s3cret"));
}

#[test]
fn alt_s_from_notes_slot_confirms() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    for c in "notes".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    for _ in 0..6 {
        form.handle_key(KeyCode::Char('j')); // → 6 = notes
    }
    form.handle_key(KeyCode::Enter); // apply the selection
    form.handle_key(KeyCode::Tab); // → value slot (textarea)
    for c in "Hi".chars() {
        form.handle_key(KeyCode::Char(c));
    }

    let FormAction::Confirm(entry) = alt_s(&mut form) else {
        panic!("Alt-s in the Notes textarea should confirm the form");
    };
    assert_eq!(entry.fields[0].field_type, FieldType::Notes);
    assert!(
        matches!(&entry.fields[0].value, FieldValue::Text(v) if v == "Hi"),
        "Alt-s must confirm, not insert a newline, got: {:?}",
        entry.fields[0].value
    );
}

#[test]
fn alt_s_with_validation_errors_keeps_the_form_open() {
    let mut form = add_form(); // empty name
    add_field(&mut form); // empty key

    assert!(
        matches!(alt_s(&mut form), FormAction::ValidationError(_)),
        "Alt-s must report validation errors instead of confirming"
    );
}
