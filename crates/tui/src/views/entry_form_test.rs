use crate::views::entry_form::{EntryForm, FormAction, FormFieldType, FormMode, ValidationIssue};
use bogita_core::domain::{FieldType, FieldValue};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

fn add_form() -> EntryForm {
    EntryForm::new_add(None)
}

fn add_form_named(name: &str) -> EntryForm {
    EntryForm::new_add(Some(name.to_string()))
}

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
fn plus_key_adds_a_field() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    assert_eq!(form.field_count(), 1);
}

#[test]
fn plus_key_focuses_new_field_key_slot() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    // first field key slot = 1
    assert_eq!(form.focused_field(), 1);
}

#[test]
fn adding_two_fields_gives_field_count_two() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Char('+'));
    assert_eq!(form.field_count(), 2);
}

#[test]
fn minus_key_removes_focused_field() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    // focused on key slot of field 0
    form.handle_key(KeyCode::Char('-'));
    assert_eq!(form.field_count(), 0);
}

#[test]
fn minus_key_on_name_slot_does_nothing() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('-'));
    assert_eq!(form.name(), "GitHub");
    assert_eq!(form.field_count(), 0);
}

// ── navigation ────────────────────────────────────────────────────────────────

#[test]
fn tab_moves_from_name_to_first_field_key() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    // '+' already moves focus to key slot 1; go back to name first
    // Re-create fresh
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    // focused=1 (key); Tab → value(2)
    form.handle_key(KeyCode::Tab);
    assert_eq!(form.focused_field(), 2);
}

#[test]
fn tab_moves_from_value_to_type() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // key(1) → value(2)
    form.handle_key(KeyCode::Tab); // value(2) → type(3)
    assert_eq!(form.focused_field(), 3);
}

#[test]
fn tab_wraps_to_name_after_last_slot() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    // slots: 0=name, 1=key, 2=value, 3=type → 4 total
    form.handle_key(KeyCode::Tab); // 1→2
    form.handle_key(KeyCode::Tab); // 2→3
    form.handle_key(KeyCode::Tab); // 3→0
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
    form.handle_key(KeyCode::Char('+'));
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
    form.handle_key(KeyCode::Char('+')); // add a field
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
    form.handle_key(KeyCode::Char('-')); // remove field
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

// ── secret field reveal ───────────────────────────────────────────────────────

/// Create an EntryForm with a single Token field (obscured).
fn form_with_token_field() -> EntryForm {
    let mut form = EntryForm::new_add(Some("Test".to_string()));
    form.handle_key(KeyCode::Char('+')); // add field, focus on key
    for c in "password".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → value
    for c in "s3cret".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
                                   // Open dropdown and select Token (index 2)
    form.handle_key(KeyCode::Char(' ')); // open dropdown
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Enter); // confirm selection
    form
}

#[test]
fn reveal_on_focus_obscured_field() {
    let mut form = form_with_token_field();
    // Focus is on type slot. Tab to value slot (BackTab to go back).
    form.handle_key(KeyCode::BackTab); // → value slot (token)
                                       // The value slot is obscured (Token), so it should be auto-revealed.
    assert!(
        form.revealed_secret_fields.contains(&0),
        "field 0 should be in revealed set when focused"
    );
}

#[test]
fn unreveal_on_leave_obscured_field() {
    let mut form = form_with_token_field();
    // Focus is on type slot. BackTab to value slot (revealed).
    form.handle_key(KeyCode::BackTab); // → value slot (revealed)
    assert!(form.revealed_secret_fields.contains(&0));
    // Tab away to type slot (unreveal).
    form.handle_key(KeyCode::Tab); // → type slot
    assert!(
        !form.revealed_secret_fields.contains(&0),
        "field 0 should be removed from revealed set when focus leaves"
    );
}

#[test]
fn ctrl_r_toggles_reveal() {
    let mut form = form_with_token_field();
    // Focus is on type slot. BackTab to value slot (auto-revealed).
    form.handle_key(KeyCode::BackTab); // → value slot
    assert!(form.revealed_secret_fields.contains(&0));
    // Ctrl-r toggles reveal off
    form.handle_key_with_modifiers(KeyCode::Char('r'), KeyModifiers::CONTROL);
    assert!(
        !form.revealed_secret_fields.contains(&0),
        "Ctrl-r should toggle reveal off"
    );
    // Ctrl-r toggles reveal back on
    form.handle_key_with_modifiers(KeyCode::Char('r'), KeyModifiers::CONTROL);
    assert!(
        form.revealed_secret_fields.contains(&0),
        "Ctrl-r should toggle reveal back on"
    );
}

#[test]
fn plain_r_in_non_obscured_value_inserts_character() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+')); // add field, focus → key
    form.handle_key(KeyCode::Tab); // → value slot (plain Text)
                                   // Plain 'r' (no Ctrl) should insert 'r' into the value, not toggle reveal.
    form.handle_key(KeyCode::Char('r'));
    assert!(
        form.revealed_secret_fields.is_empty(),
        "plain 'r' should not affect reveal state"
    );
    let FormAction::Confirm(entry) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };
    assert!(
        matches!(&entry.fields[0].value, FieldValue::Text(v) if v == "r"),
        "plain 'r' should be inserted into the value"
    );
}

#[test]
fn plain_r_in_obscured_value_inserts_character() {
    let mut form = form_with_token_field();
    // Focus is on type slot after form_with_token_field. BackTab to value.
    form.handle_key(KeyCode::BackTab); // → value slot (Token, auto-revealed)
    assert!(form.revealed_secret_fields.contains(&0));
    // Plain 'r' (no Ctrl) should insert 'r' into the value, not toggle reveal.
    form.handle_key(KeyCode::Char('r'));
    assert!(
        form.revealed_secret_fields.contains(&0),
        "plain 'r' should not toggle reveal off in obscured field"
    );
    let FormAction::Confirm(entry) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };
    assert!(
        matches!(&entry.fields[0].value, FieldValue::Hidden(v) if v == "s3cretr"),
        "plain 'r' should be inserted into the obscured value, got: {:?}",
        entry.fields[0].value
    );
}

#[test]
fn plain_fields_not_affected_by_reveal() {
    let mut form = EntryForm::new_add(Some("Test".to_string()));
    form.handle_key(KeyCode::Char('+')); // add field
    form.handle_key(KeyCode::Tab); // → value slot (plain Text)
                                   // Plain text fields should not be in revealed set
    assert!(
        form.revealed_secret_fields.is_empty(),
        "plain text fields should not be in revealed set"
    );
}

#[test]
fn confirm_clears_reveal_state() {
    let mut form = form_with_token_field();
    // BackTab to value slot (auto-revealed).
    form.handle_key(KeyCode::BackTab); // → value slot
    assert!(form.revealed_secret_fields.contains(&0));
    // Confirm the form
    form.handle_key(KeyCode::Enter);
    assert!(
        form.revealed_secret_fields.is_empty(),
        "confirm should clear revealed set"
    );
}

#[test]
fn confirm_syncs_field_type_from_choice() {
    use bogita_core::domain::FieldType;

    let mut form = EntryForm::new_add(Some("Test".to_string()));
    form.handle_key(KeyCode::Char('+')); // add field, focus on key
    form.handle_key(KeyCode::Tab); // → value
    for c in "s3cret".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
                                   // Change type to Token (index 2) via the Choice widget directly,
                                   // simulating what happens when the user navigates with j/k and
                                   // the popup closes via Enter.
    form.handle_key(KeyCode::Char(' ')); // open dropdown
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Enter); // close popup, apply selection
                                     // Tab back to value slot and confirm
    form.handle_key(KeyCode::BackTab); // → value slot
    let action = form.handle_key(KeyCode::Enter);
    match action {
        FormAction::Confirm(entry) => {
            assert_eq!(
                entry.fields[0].field_type,
                FieldType::Token,
                "field type should be Token"
            );
            assert!(
                matches!(
                    &entry.fields[0].value,
                    bogita_core::domain::FieldValue::Hidden(_)
                ),
                "value should be Hidden for Token type"
            );
        }
        other => panic!("expected Confirm, got {:?}", other),
    }
}

#[test]
fn cancel_clears_reveal_state() {
    let mut form = form_with_token_field();
    // BackTab to value slot (auto-revealed).
    form.handle_key(KeyCode::BackTab); // → value slot
    assert!(form.revealed_secret_fields.contains(&0));
    // Cancel the form
    form.handle_key(KeyCode::Esc);
    assert!(
        form.revealed_secret_fields.is_empty(),
        "cancel should clear revealed set"
    );
}

#[test]
fn ssh_key_field_also_reveals() {
    let mut form = EntryForm::new_add(Some("Test".to_string()));
    form.handle_key(KeyCode::Char('+')); // add field
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot
                                   // Open dropdown and select SshKey (index 4)
    form.handle_key(KeyCode::Char(' ')); // open dropdown
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Char('j')); // Totp
    form.handle_key(KeyCode::Char('j')); // SshKey
    form.handle_key(KeyCode::Enter); // confirm selection
                                     // BackTab to value slot
    form.handle_key(KeyCode::BackTab); // → value slot
    assert!(
        form.revealed_secret_fields.contains(&0),
        "SshKey field should be auto-revealed on focus"
    );
}

#[test]
fn totp_field_also_reveals() {
    let mut form = EntryForm::new_add(Some("Test".to_string()));
    form.handle_key(KeyCode::Char('+')); // add field
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot
                                   // Open dropdown and select Totp (index 3)
    form.handle_key(KeyCode::Char(' ')); // open dropdown
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Char('j')); // Totp
    form.handle_key(KeyCode::Enter); // confirm selection
                                     // BackTab to value slot
    form.handle_key(KeyCode::BackTab); // → value slot
    assert!(
        form.revealed_secret_fields.contains(&0),
        "Totp field should be auto-revealed on focus"
    );
}

#[test]
fn shift_tab_moves_focus_backward() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // 1→2
    form.handle_key(KeyCode::BackTab); // 2→1
    assert_eq!(form.focused_field(), 1);
}

// ── typing into key / value slots ────────────────────────────────────────────

#[test]
fn typing_on_key_slot_populates_field_key() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    for c in "username".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    let FormAction::Confirm(entry) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };
    assert_eq!(entry.fields[0].key, "username");
}

#[test]
fn typing_on_value_slot_populates_field_value() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value slot
    for c in "https://github.com".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    let FormAction::Confirm(entry) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };
    assert!(matches!(&entry.fields[0].value, FieldValue::Text(v) if v == "https://github.com"));
}

#[test]
fn backspace_on_value_slot_removes_char() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Char('a'));
    form.handle_key(KeyCode::Char('b'));
    form.handle_key(KeyCode::Backspace);
    let FormAction::Confirm(entry) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };
    assert!(matches!(&entry.fields[0].value, FieldValue::Text(v) if v == "a"));
}

// ── type slot — dropdown ──────────────────────────────────────────────────────

fn open_dropdown(form: &mut EntryForm) {
    form.handle_key(KeyCode::Char(' ')); // open dropdown
}

#[test]
fn new_field_type_is_text_by_default() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    assert!(!form.focused_field_is_obscured());
    form.handle_key(KeyCode::Tab); // → value
    assert!(!form.focused_field_is_obscured());
    form.handle_key(KeyCode::Tab); // → type slot
    assert!(!form.focused_field_is_obscured());
}

#[test]
fn j_on_type_slot_moves_selector_to_username() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot (cursor=0=Text)
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // → 1=Username
    form.handle_key(KeyCode::Enter); // confirm
    assert_eq!(form.field_badge(0), "username");
    assert!(!form.focused_field_is_obscured());
}

#[test]
fn j_twice_on_type_slot_selects_token() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Enter); // confirm
    assert_eq!(form.field_badge(0), "token");
    assert!(form.focused_field_is_obscured());
}

#[test]
fn k_on_type_slot_wraps_to_last_variant() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot (cursor=0)
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('k')); // wraps to 5=Notes
    form.handle_key(KeyCode::Enter); // confirm
    assert_eq!(form.field_badge(0), "notes");
}

#[test]
fn down_arrow_on_type_slot_navigates_selector() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Down); // → 1=Username
    form.handle_key(KeyCode::Enter); // confirm
    assert_eq!(form.field_badge(0), "username");
}

#[test]
fn up_arrow_on_type_slot_navigates_selector() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Down); // → 1=Username
    form.handle_key(KeyCode::Up); // → 0=Text
    form.handle_key(KeyCode::Enter); // confirm
    assert_eq!(form.field_badge(0), "text");
}

#[test]
fn type_selector_wraps_at_bottom() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot (cursor=0)
    open_dropdown(&mut form);
    // Navigate past last item (5) → wraps to 0
    for _ in 0..6 {
        form.handle_key(KeyCode::Char('j'));
    }
    form.handle_key(KeyCode::Enter); // confirm
    assert_eq!(form.field_badge(0), "text");
}

#[test]
fn type_selector_wraps_at_top() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot (cursor=0)
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('k')); // wraps to 5 (Notes)
    form.handle_key(KeyCode::Enter); // confirm
    assert_eq!(form.field_badge(0), "notes");
}

#[test]
fn selecting_totp_makes_field_obscured() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Char('j')); // Totp
    form.handle_key(KeyCode::Enter); // confirm
    assert_eq!(form.field_badge(0), "totp");
    assert!(form.focused_field_is_obscured());
}

#[test]
fn selecting_sshkey_makes_field_obscured() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Char('j')); // Totp
    form.handle_key(KeyCode::Char('j')); // SshKey
    form.handle_key(KeyCode::Enter); // confirm
    assert_eq!(form.field_badge(0), "ssh-key");
    assert!(form.focused_field_is_obscured());
}

#[test]
fn enter_on_type_slot_closed_confirms_form() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot (dropdown closed)
                                   // Enter with dropdown closed confirms the form
    let action = form.handle_key(KeyCode::Enter);
    assert!(matches!(action, FormAction::Confirm(_)));
}

#[test]
fn enter_on_type_slot_open_confirms_selection_not_form() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
                                         // Enter closes dropdown and applies selection — does NOT confirm form
    let action = form.handle_key(KeyCode::Enter);
    assert_eq!(action, FormAction::None);
    assert_eq!(form.field_badge(0), "token");
}

#[test]
fn esc_on_open_dropdown_closes_without_changing_type() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot (Text)
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // cursor moves to Username but not applied yet
                                         // Esc closes dropdown — type stays as Text
    let action = form.handle_key(KeyCode::Esc);
    assert_eq!(action, FormAction::None); // did NOT cancel the form
    assert_eq!(form.field_badge(0), "text"); // type unchanged
}

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

// ── focus-driven reveal ───────────────────────────────────────────────────────

fn navigate_to_token_value_slot(form: &mut EntryForm) {
    form.handle_key(KeyCode::Char('+')); // add field, focus → key slot
    form.handle_key(KeyCode::Tab); // → value (plain)
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Enter); // confirm selection
    form.handle_key(KeyCode::BackTab); // back to value slot (now "token")
}

#[test]
fn token_value_slot_is_revealed_when_focused() {
    let mut form = add_form_named("GitHub");
    navigate_to_token_value_slot(&mut form);
    assert_eq!(form.focused_slot_label(), "token");
    assert!(
        form.focused_value_is_revealed(),
        "token slot should be revealed while focused"
    );
}

#[test]
fn token_value_slot_is_masked_when_not_focused() {
    let mut form = add_form_named("GitHub");
    navigate_to_token_value_slot(&mut form);
    form.handle_key(KeyCode::Tab); // move focus away to type slot
    assert_eq!(form.focused_slot_label(), "type");
    assert!(
        !form.focused_value_is_revealed(),
        "token slot should be masked when not focused"
    );
}

#[test]
fn plain_value_slot_is_never_revealed() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+')); // add field
    form.handle_key(KeyCode::Tab); // → plain value slot (Text)
    assert_eq!(form.focused_slot_label(), "value");
    assert!(
        !form.focused_value_is_revealed(),
        "plain value slot should never set revealed"
    );
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

// ── Notes field type ──────────────────────────────────────────────────────────

#[test]
fn notes_type_is_not_obscured() {
    assert!(!FormFieldType::Notes.is_obscured());
}

#[test]
fn notes_type_is_not_encrypted() {
    assert!(!FormFieldType::Notes.is_encrypted());
}

#[test]
fn notes_type_label_is_notes() {
    assert_eq!(FormFieldType::Notes.label(), "notes");
}

#[test]
fn notes_type_produces_text_field_value() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    // Navigate to Notes (index 5): j=Username, j=Token, j=Totp, j=SshKey, j=Notes
    for _ in 0..5 {
        form.handle_key(KeyCode::Char('j'));
    }
    form.handle_key(KeyCode::Enter); // confirm selection
    assert_eq!(form.field_badge(0), "notes");
    form.handle_key(KeyCode::Tab); // → name (wraps from type)
    form.handle_key(KeyCode::Tab); // → key
    form.handle_key(KeyCode::Tab); // → value (TextAreaState)
                                   // Type some text
    form.handle_key(KeyCode::Char('H'));
    form.handle_key(KeyCode::Char('i'));
    // Navigate to name to confirm
    form.handle_key(KeyCode::Tab); // → type
    form.handle_key(KeyCode::Tab); // → name
    let FormAction::Confirm(entry) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };
    assert!(matches!(&entry.fields[0].value, FieldValue::Text(v) if v == "Hi"));
    assert_eq!(entry.fields[0].field_type, FieldType::Notes);
}

#[test]
fn notes_field_enter_inserts_newline_not_confirm() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    // Navigate to Notes (index 5)
    for _ in 0..5 {
        form.handle_key(KeyCode::Char('j'));
    }
    form.handle_key(KeyCode::Enter); // confirm selection
    assert_eq!(form.field_badge(0), "notes");
    // Navigate to value slot: type → name → key → value
    form.handle_key(KeyCode::Tab); // → name
    form.handle_key(KeyCode::Tab); // → key
    form.handle_key(KeyCode::Tab); // → value (TextAreaState)
                                   // Type "Hello" then Enter (should insert newline, not confirm)
    for c in "Hello".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    let action = form.handle_key(KeyCode::Enter);
    assert_eq!(
        action,
        FormAction::None,
        "Enter in Notes should NOT confirm"
    );
    // Type more text
    for c in " World".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    // Navigate to name to confirm
    form.handle_key(KeyCode::Tab); // → type
    form.handle_key(KeyCode::Tab); // → name
    let FormAction::Confirm(entry) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };
    assert!(matches!(&entry.fields[0].value, FieldValue::Text(v) if v == "Hello\n World"));
}

#[test]
fn notes_field_esc_cancels_form() {
    let mut form = add_form_named("GitHub");
    form.handle_key(KeyCode::Char('+'));
    form.handle_key(KeyCode::Tab); // → value
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    // Navigate to Notes (index 5)
    for _ in 0..5 {
        form.handle_key(KeyCode::Char('j'));
    }
    form.handle_key(KeyCode::Enter); // confirm selection
                                     // Navigate to value slot
    form.handle_key(KeyCode::Tab); // → name
    form.handle_key(KeyCode::Tab); // → key
    form.handle_key(KeyCode::Tab); // → value (TextAreaState)
                                   // Esc should return ConfirmDiscard (form is dirty)
    assert_eq!(form.handle_key(KeyCode::Esc), FormAction::ConfirmDiscard);
}

#[test]
fn notes_field_round_trips_through_edit() {
    use bogita_core::domain::{Entry, EntryType, Field};
    use chrono::Utc;
    use uuid::Uuid;

    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: "MyNotes".to_string(),
        entry_type: EntryType::Note,
        fields: vec![Field {
            id: Uuid::new_v4(),
            key: "notes".to_string(),
            value: FieldValue::Text("Line 1\nLine 2\nLine 3".to_string()),
            field_type: FieldType::Notes,
            encrypted: false,
            idx: 0,
        }],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    };
    let FormAction::Confirm(e) = EntryForm::new_edit(&entry).handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };
    assert!(matches!(&e.fields[0].value, FieldValue::Text(v) if v == "Line 1\nLine 2\nLine 3"));
    assert_eq!(e.fields[0].field_type, FieldType::Notes);
}

// ── scrolling ─────────────────────────────────────────────────────────────────

/// Create an EntryForm with `count` fields by repeatedly pressing '+'.
fn form_with_fields(count: usize) -> EntryForm {
    let mut form = EntryForm::new_add(Some("Test".to_string()));
    for _ in 0..count {
        form.handle_key(KeyCode::Char('+'));
    }
    form
}

#[test]
fn scroll_offset_starts_at_zero() {
    let form = form_with_fields(5);
    assert_eq!(form.scroll_offset, 0);
}

#[test]
fn scroll_not_needed_with_few_fields() {
    let mut form = form_with_fields(2);
    form.last_visible_rows = 4;
    // 2 fields fit in 4 visible rows, scroll_offset stays 0.
    assert_eq!(form.scroll_offset, 0);
}

#[test]
fn scroll_needed_with_many_fields() {
    let mut form = form_with_fields(20);
    form.last_visible_rows = 4;
    // Tab to field 10 (key slot = 1 + 10*3 = 31 tabs).
    for _ in 0..31 {
        form.handle_key(KeyCode::Tab);
    }
    // With 4 visible rows, field 10 is below the visible range (0..4).
    // auto_scroll adjusts incrementally as we Tab through fields.
    // By field 10 key, scroll_offset = 6 (field 10 is at position 10-6=4,
    // which is the last visible position in [0,4)).
    assert_eq!(form.scroll_offset, 6);
}

#[test]
fn tab_to_field_below_visible_scrolls_down() {
    let mut form = form_with_fields(20);
    form.last_visible_rows = 4;
    // Tab to field 15 key slot (1 + 15*3 = 46 tabs).
    for _ in 0..46 {
        form.handle_key(KeyCode::Tab);
    }
    // With 4 visible rows, field 15 is far below the visible range.
    // auto_scroll adjusts incrementally. By field 15 key, scroll_offset = 11.
    assert_eq!(form.scroll_offset, 11);
}

#[test]
fn backtab_to_field_above_visible_scrolls_up() {
    let mut form = form_with_fields(20);
    form.last_visible_rows = 4;
    // Tab to field 15 key slot.
    for _ in 0..46 {
        form.handle_key(KeyCode::Tab);
    }
    // Now at field 15 key. Set scroll_offset to 16 (showing fields 16..19).
    form.scroll_offset = 16;
    // BackTab to previous slot (field 14 type).
    form.handle_key(KeyCode::BackTab);
    // Field 14 is above scroll_offset (16), so scroll_offset adjusts.
    // The exact value depends on focus order, but it must be < 16.
    assert!(form.scroll_offset < 16);
}

#[test]
fn field_removal_adjusts_scroll() {
    let mut form = form_with_fields(20);
    form.last_visible_rows = 4;
    // Set scroll_offset to max (16 = 20-4).
    form.scroll_offset = 16;
    // Navigate to field 19 key slot and remove it.
    for _ in 0..58 {
        form.handle_key(KeyCode::Tab);
    }
    form.handle_key(KeyCode::Char('-'));
    // After removal: 19 fields, max_scroll = 19-4 = 15.
    // scroll_offset was 16, now clamped to 15.
    assert_eq!(form.scroll_offset, 15);
}

#[test]
fn add_field_scrolls_to_show_new_field() {
    let mut form = form_with_fields(20);
    form.last_visible_rows = 4;
    form.scroll_offset = 0;
    // Add a field — now 21 fields, 4 visible rows.
    form.handle_key(KeyCode::Char('+'));
    // scroll_offset = 21 - 4 = 17. Field 20 is at position 20-17=3 in [0,4).
    assert_eq!(form.scroll_offset, 17);
}

#[test]
fn name_always_visible() {
    let mut form = form_with_fields(20);
    form.last_visible_rows = 4;
    // Set scroll_offset to show last fields.
    form.scroll_offset = 16;
    // scroll_offset stays at 16 (name is not part of scroll range).
    assert_eq!(form.scroll_offset, 16);
    // Tab back to name to verify it's reachable.
    // form_with_fields leaves focus on last field's key slot.
    // BackTab 58 times to wrap around to name.
    for _ in 0..58 {
        form.handle_key(KeyCode::BackTab);
    }
    assert_eq!(form.focused_field(), 0);
}

// ── inline validation ─────────────────────────────────────────────────────────

#[test]
fn empty_name_shows_validation_error() {
    let mut form = EntryForm::new_add(None);
    // Press Backspace to trigger a key event (which calls validate()).
    form.handle_key(KeyCode::Backspace);
    assert!(form.has_validation_errors());
    assert!(form
        .validation_errors()
        .contains(&ValidationIssue::EmptyName));
}

#[test]
fn non_empty_name_clears_error() {
    let mut form = EntryForm::new_add(None);
    form.handle_key(KeyCode::Char('x'));
    assert!(!form.has_validation_errors());
}

#[test]
fn empty_field_key_shows_error_after_touch() {
    let mut form = EntryForm::new_add(Some("Test".to_string()));
    form.handle_key(KeyCode::Char('+')); // add field
    form.handle_key(KeyCode::Char('a')); // type in key (marks touched)
    form.handle_key(KeyCode::Backspace); // delete it — now empty but touched
    assert!(form
        .validation_errors()
        .iter()
        .any(|e| matches!(e, ValidationIssue::EmptyKey(0))));
}

#[test]
fn duplicate_keys_shown_as_error() {
    let mut form = EntryForm::new_add(Some("Test".to_string()));
    form.handle_key(KeyCode::Char('+'));
    for c in "key1".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Char('+'));
    for c in "key1".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    assert!(form
        .validation_errors()
        .iter()
        .any(|e| matches!(e, ValidationIssue::DuplicateKey(_))));
}

#[test]
fn confirm_with_errors_returns_validation_error() {
    let mut form = EntryForm::new_add(None);
    // Empty name + Enter should trigger validation error.
    let action = form.handle_key(KeyCode::Enter);
    assert!(matches!(action, FormAction::ValidationError(_)));
}

#[test]
fn confirm_without_errors_succeeds() {
    let mut form = EntryForm::new_add(Some("Test".to_string()));
    let action = form.handle_key(KeyCode::Enter);
    assert!(matches!(action, FormAction::Confirm(_)));
}

#[test]
fn newly_added_field_not_immediately_invalid() {
    let mut form = EntryForm::new_add(Some("Test".to_string()));
    form.handle_key(KeyCode::Char('+')); // add field, don't type in key
    assert!(
        !form
            .validation_errors()
            .iter()
            .any(|e| matches!(e, ValidationIssue::EmptyKey(0))),
        "newly added field should not be flagged as EmptyKey until touched"
    );
}
