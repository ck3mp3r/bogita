use crate::views::entry_form::{EntryForm, FormAction, FormMode};
use ratatui::crossterm::event::KeyCode;

use super::support::*;

// ── construction ──────────────────────────────────────────────────────────────

#[test]
fn new_add_form_mode_is_add() {
    assert_eq!(add_form().mode(), FormMode::Add);
}

#[test]
fn new_add_form_name_is_empty_by_default() {
    assert_eq!(add_form().name(), "");
}

#[test]
fn new_add_form_pre_fills_name() {
    assert_eq!(add_form_named("GitHub").name(), "GitHub");
}

#[test]
fn new_add_form_has_no_fields_by_default() {
    assert_eq!(add_form().field_count(), 0);
}

// ── name editing ──────────────────────────────────────────────────────────────

#[test]
fn typing_appends_to_name_when_focused_on_name() {
    let mut form = add_form();
    form.handle_key(KeyCode::Char('G'));
    form.handle_key(KeyCode::Char('H'));
    assert_eq!(form.name(), "GH");
}

#[test]
fn backspace_removes_last_char_from_name() {
    let mut form = add_form_named("GH");
    form.handle_key(KeyCode::Backspace);
    assert_eq!(form.name(), "G");
}

#[test]
fn backspace_on_empty_name_does_not_crash() {
    let mut form = add_form();
    form.handle_key(KeyCode::Backspace);
    assert_eq!(form.name(), "");
}

// ── validation ────────────────────────────────────────────────────────────────

#[test]
fn enter_with_empty_name_returns_validation_error() {
    let mut form = add_form();
    assert!(matches!(
        form.handle_key(KeyCode::Enter),
        FormAction::ValidationError(_)
    ));
}

#[test]
fn enter_with_valid_name_returns_confirm() {
    let mut form = add_form_named("GitHub");
    match form.handle_key(KeyCode::Enter) {
        FormAction::Confirm(entry) => assert_eq!(entry.name, "GitHub"),
        other => panic!("expected Confirm, got {:?}", other),
    }
}

// ── add / remove fields ───────────────────────────────────────────────────────

#[test]
fn alt_n_adds_a_field() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    assert_eq!(form.field_count(), 1);
}

#[test]
fn alt_n_focuses_new_field_key_slot() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    // first field key slot = 1
    assert_eq!(form.focused_field(), 1);
}

#[test]
fn adding_two_fields_gives_field_count_two() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    add_field(&mut form);
    assert_eq!(form.field_count(), 2);
}

#[test]
fn alt_d_removes_focused_field() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    // focused on key slot of field 0
    remove_field(&mut form);
    assert_eq!(form.field_count(), 0);
}

#[test]
fn alt_d_on_name_slot_does_nothing() {
    let mut form = add_form_named("GitHub");
    remove_field(&mut form);
    assert_eq!(form.name(), "GitHub");
    assert_eq!(form.field_count(), 0);
}

#[test]
fn plus_and_minus_type_into_the_name() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Char('-'));
    assert_eq!(form.name(), "GitHub+-");
    assert_eq!(form.field_count(), 0);
}

#[test]
fn space_types_into_the_name() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char(' '));
    assert_eq!(form.name(), "GitHub ");
}

#[test]
fn bare_g_types_into_the_key_slot() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    form.handle_key(KeyCode::Char('g'));
    assert_eq!(form.fields[0].key_state.text(), "g");
}

// ── navigation ────────────────────────────────────────────────────────────────

#[test]
fn tab_moves_from_name_to_first_field_key() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    // Alt-n already moves focus to key slot 1; go back to name first
    form.handle_key(KeyCode::BackTab);
    assert_eq!(form.focused_field(), 0);
    // Tab from the name slot lands on the key slot of the first field.
    form.handle_key(KeyCode::Tab);
    assert_eq!(form.focused_field(), 1);
}

#[test]
fn tab_moves_from_key_to_type() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    form.handle_key(KeyCode::Tab); // key(1) → type(3)
    assert_eq!(form.focused_field(), 3);
}

#[test]
fn tab_moves_from_type_to_value() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    form.handle_key(KeyCode::Tab); // key(1) → type(3)
    form.handle_key(KeyCode::Tab); // type(3) → value(2)
    assert_eq!(form.focused_field(), 2);
}

#[test]
fn tab_wraps_to_name_after_last_slot() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    // slots: 0=name, 1=key, 3=type, 2=value → 4 total
    form.handle_key(KeyCode::Tab); // 1→3
    form.handle_key(KeyCode::Tab); // 3→2
    form.handle_key(KeyCode::Tab); // 2→0
    assert_eq!(form.focused_field(), 0);
}

// ── dirty state tracking ──────────────────────────────────────────────────────

#[test]
fn add_mode_not_dirty_when_empty() {
    let form = add_form();
    assert!(!form.is_dirty());
}

#[test]
fn add_mode_dirty_when_name_entered() {
    let form = add_form_named("GitHub");
    assert!(form.is_dirty());
}

#[test]
fn add_mode_dirty_when_field_added() {
    let mut form = add_form();
    add_field(&mut form);
    assert!(form.is_dirty());
}

#[test]
fn edit_mode_not_dirty_when_unchanged() {
    use bogita_core::domain::{Entry, EntryType, Field, FieldType, FieldValue};
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
            field_type: FieldType::Token,
            value: FieldValue::Hidden("s3cret".to_string()),
            encrypted: true,
            idx: 0,
        }],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    };
    let form = EntryForm::new_edit(&entry);
    assert!(
        !form.is_dirty(),
        "edit form with no changes should not be dirty"
    );
}

#[test]
fn edit_mode_dirty_when_name_changed() {
    use bogita_core::domain::{Entry, EntryType, Field, FieldType, FieldValue};
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
            field_type: FieldType::Token,
            value: FieldValue::Hidden("s3cret".to_string()),
            encrypted: true,
            idx: 0,
        }],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    };
    let mut form = EntryForm::new_edit(&entry);
    // Change the name
    form.handle_key(KeyCode::Backspace);
    form.handle_key(KeyCode::Backspace);
    form.handle_key(KeyCode::Backspace);
    form.handle_key(KeyCode::Backspace);
    form.handle_key(KeyCode::Backspace);
    form.handle_key(KeyCode::Backspace);
    form.handle_key(KeyCode::Backspace);
    for c in "Renamed".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    assert!(form.is_dirty());
}

#[test]
fn edit_mode_dirty_when_field_value_changed() {
    use bogita_core::domain::{Entry, EntryType, Field, FieldType, FieldValue};
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
            field_type: FieldType::Token,
            value: FieldValue::Hidden("s3cret".to_string()),
            encrypted: true,
            idx: 0,
        }],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    };
    let mut form = EntryForm::new_edit(&entry);
    // Tab to value slot and change the value
    form.handle_key(KeyCode::Tab); // → key
    form.handle_key(KeyCode::Tab); // → type
    form.handle_key(KeyCode::Tab); // → value
                                   // Clear "s3cret" and type new value
    for _ in 0.."s3cret".len() {
        form.handle_key(KeyCode::Backspace);
    }
    for c in "n3wpass".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    assert!(form.is_dirty());
}

#[test]
fn edit_mode_dirty_when_field_added() {
    use bogita_core::domain::{Entry, EntryType, Field, FieldType, FieldValue};
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
            field_type: FieldType::Token,
            value: FieldValue::Hidden("s3cret".to_string()),
            encrypted: true,
            idx: 0,
        }],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    };
    let mut form = EntryForm::new_edit(&entry);
    add_field(&mut form); // add a field
    assert!(form.is_dirty());
}

#[test]
fn edit_mode_dirty_when_field_removed() {
    use bogita_core::domain::{Entry, EntryType, Field, FieldType, FieldValue};
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
            field_type: FieldType::Token,
            value: FieldValue::Hidden("s3cret".to_string()),
            encrypted: true,
            idx: 0,
        }],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    };
    let mut form = EntryForm::new_edit(&entry);
    // Tab to key slot then remove the field
    form.handle_key(KeyCode::Tab); // → key
    remove_field(&mut form); // remove field
    assert!(form.is_dirty());
}

#[test]
fn esc_when_not_dirty_cancels_immediately() {
    let mut form = add_form(); // empty add form — not dirty
    assert_eq!(form.handle_key(KeyCode::Esc), FormAction::Cancel);
}

#[test]
fn esc_when_dirty_returns_confirm_discard() {
    let mut form = add_form_named("GitHub"); // has a name — dirty
    assert_eq!(form.handle_key(KeyCode::Esc), FormAction::ConfirmDiscard);
}
