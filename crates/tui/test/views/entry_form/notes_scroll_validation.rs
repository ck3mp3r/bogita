use crate::views::entry_form::{EntryForm, FormAction, FormFieldType, ValidationIssue};
use bogita_core::domain::{FieldType, FieldValue};
use ratatui::crossterm::event::KeyCode;

use super::support::*;

// ── focus-driven reveal ───────────────────────────────────────────────────────

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
