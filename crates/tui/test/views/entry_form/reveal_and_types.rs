use crate::views::entry_form::{EntryForm, FormAction};
use bogita_core::domain::{
    agent_servable_field, Entry, EntryType, FieldType, FieldValue, AGENT_SERVABLE_KEY,
};
use chrono::Utc;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use uuid::Uuid;

use super::support::*;

// ── secret field reveal ───────────────────────────────────────────────────────

#[test]
fn reveal_on_focus_obscured_field() {
    let mut form = form_with_token_field();
    // Focus is on type slot. Tab to value slot (BackTab to go back).
    form.handle_key(KeyCode::Tab); // → value slot (token)
                                   // The value slot is obscured (Token), so it should be auto-revealed.
    assert!(
        form.revealed_secret_fields.contains(&0),
        "field 0 should be in revealed set when focused"
    );
}

#[test]
fn unreveal_on_leave_obscured_field() {
    let mut form = form_with_token_field();
    // Focus is on type slot. Tab to value slot (revealed).
    form.handle_key(KeyCode::Tab); // → value slot (revealed)
    assert!(form.revealed_secret_fields.contains(&0));
    // BackTab to the type slot (unreveal).
    form.handle_key(KeyCode::BackTab); // → type slot
    assert!(
        !form.revealed_secret_fields.contains(&0),
        "field 0 should be removed from revealed set when focus leaves"
    );
}

#[test]
fn alt_r_toggles_reveal() {
    let mut form = form_with_token_field();
    // Focus is on type slot. BackTab to value slot (auto-revealed).
    form.handle_key(KeyCode::Tab); // → value slot
    assert!(form.revealed_secret_fields.contains(&0));
    // Alt-r toggles reveal off
    form.handle_key_with_modifiers(KeyCode::Char('r'), KeyModifiers::ALT);
    assert!(
        !form.revealed_secret_fields.contains(&0),
        "Alt-r should toggle reveal off"
    );
    // Alt-r toggles reveal back on
    form.handle_key_with_modifiers(KeyCode::Char('r'), KeyModifiers::ALT);
    assert!(
        form.revealed_secret_fields.contains(&0),
        "Alt-r should toggle reveal back on"
    );
}

#[test]
fn plain_r_in_non_obscured_value_inserts_character() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form); // add field, focus → key
    form.handle_key(KeyCode::Tab); // → type slot
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
    form.handle_key(KeyCode::Tab); // → value slot (Token, auto-revealed)
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
    add_field(&mut form); // add field
    form.handle_key(KeyCode::Tab); // → type slot
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
    form.handle_key(KeyCode::Tab); // → value slot
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
    add_field(&mut form); // add field, focus on key
    form.handle_key(KeyCode::Tab); // → type slot
                                   // Change type to Token (index 2) via the Choice widget directly,
                                   // simulating what happens when the user navigates with j/k and
                                   // the popup closes via Enter.
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Enter); // close popup, apply selection
                                     // Tab to the value slot and confirm
    form.handle_key(KeyCode::Tab); // → value slot
    for c in "s3cret".chars() {
        form.handle_key(KeyCode::Char(c));
    }
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
    form.handle_key(KeyCode::Tab); // → value slot
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
    add_field(&mut form); // add field
    form.handle_key(KeyCode::Tab); // → type slot
                                   // Open dropdown and select SshKey (index 4)
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Char('j')); // Totp
    form.handle_key(KeyCode::Char('j')); // SshKey
    form.handle_key(KeyCode::Enter); // confirm selection
                                     // BackTab to value slot
    form.handle_key(KeyCode::Tab); // → value slot
    assert!(
        form.revealed_secret_fields.contains(&0),
        "SshKey field should be auto-revealed on focus"
    );
}

#[test]
fn totp_field_also_reveals() {
    let mut form = EntryForm::new_add(Some("Test".to_string()));
    add_field(&mut form); // add field
    form.handle_key(KeyCode::Tab); // → type slot
                                   // Open dropdown and select Totp (index 3)
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Char('j')); // Totp
    form.handle_key(KeyCode::Enter); // confirm selection
                                     // BackTab to value slot
    form.handle_key(KeyCode::Tab); // → value slot
    assert!(
        form.revealed_secret_fields.contains(&0),
        "Totp field should be auto-revealed on focus"
    );
}

#[test]
fn shift_tab_moves_focus_backward() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    form.handle_key(KeyCode::Tab); // 1→2
    form.handle_key(KeyCode::BackTab); // 2→1
    assert_eq!(form.focused_field(), 1);
}

// ── typing into key / value slots ────────────────────────────────────────────

#[test]
fn typing_on_key_slot_populates_field_key() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
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
    add_field(&mut form);
    form.handle_key(KeyCode::Tab); // → type slot
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
    add_field(&mut form);
    form.handle_key(KeyCode::Tab); // → type slot
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

#[test]
fn new_field_type_is_text_by_default() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    assert!(!form.focused_field_is_obscured());
    form.handle_key(KeyCode::Tab); // → type slot
    assert!(!form.focused_field_is_obscured());
    form.handle_key(KeyCode::Tab); // → value slot
    assert!(!form.focused_field_is_obscured());
}

#[test]
fn j_on_type_slot_moves_selector_to_username() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
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
    add_field(&mut form);
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
    add_field(&mut form);
    form.handle_key(KeyCode::Tab); // → type slot (cursor=0)
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('k')); // wraps to 7=Boolean
    form.handle_key(KeyCode::Enter); // confirm
    assert_eq!(form.field_badge(0), "boolean");
}

#[test]
fn down_arrow_on_type_slot_navigates_selector() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Down); // → 1=Username
    form.handle_key(KeyCode::Enter); // confirm
    assert_eq!(form.field_badge(0), "username");
}

#[test]
fn up_arrow_on_type_slot_navigates_selector() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
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
    add_field(&mut form);
    form.handle_key(KeyCode::Tab); // → type slot (cursor=0)
    open_dropdown(&mut form);
    // Navigate past last item (7) → wraps to 0
    for _ in 0..8 {
        form.handle_key(KeyCode::Char('j'));
    }
    form.handle_key(KeyCode::Enter); // confirm
    assert_eq!(form.field_badge(0), "text");
}

#[test]
fn type_selector_wraps_at_top() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    form.handle_key(KeyCode::Tab); // → type slot (cursor=0)
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('k')); // wraps to 6 (Boolean)
    form.handle_key(KeyCode::Enter); // confirm
    assert_eq!(form.field_badge(0), "boolean");
}

#[test]
fn selecting_totp_makes_field_obscured() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
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
    add_field(&mut form);
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
fn toggling_the_agent_servable_row_flips_the_boolean_in_place() {
    // -- Setup & Fixtures
    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: "Deploy key".to_string(),
        entry_type: EntryType::Token,
        fields: vec![agent_servable_field(true, 0)],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    };
    let mut form = EntryForm::new_edit(&entry);
    form.handle_key(KeyCode::Tab); // → key slot
    form.handle_key(KeyCode::Tab); // → type slot
    form.handle_key(KeyCode::Tab); // → value slot

    // -- Exec: Alt-t flips the sense and opens no popup.
    flip_boolean(&mut form); // true → false
    let FormAction::Confirm(confirmed) = form.handle_key(KeyCode::Enter) else {
        panic!("expected Confirm");
    };

    // -- Check
    assert_eq!(form.field_badge(0), "boolean");
    assert!(
        !form.focused_field_is_obscured(),
        "the daemon reads the gate in plaintext"
    );
    assert_eq!(confirmed.fields[0].key, AGENT_SERVABLE_KEY);
    assert_eq!(confirmed.fields[0].field_type, FieldType::Boolean);
    assert_eq!(confirmed.fields[0].value, FieldValue::Boolean(false));
    assert!(
        !confirmed.fields[0].encrypted,
        "the daemon reads the gate in plaintext"
    );
}

#[test]
fn enter_on_type_slot_closed_opens_dropdown() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    form.handle_key(KeyCode::Tab); // → type slot (dropdown closed)
                                   // Enter with dropdown closed opens the picker, it does not confirm the form
    let action = form.handle_key(KeyCode::Enter);
    assert_eq!(action, FormAction::None);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Enter); // apply
    assert_eq!(form.field_badge(0), "username");
}

#[test]
fn space_on_type_slot_closed_changes_nothing() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
    form.handle_key(KeyCode::Tab); // → type slot (dropdown closed)
    let action = form.handle_key(KeyCode::Char(' '));
    assert_eq!(action, FormAction::None);
    assert_eq!(form.field_badge(0), "text");
}

#[test]
fn enter_on_type_slot_open_confirms_selection_not_form() {
    let mut form = add_form_named("GitHub");
    add_field(&mut form);
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
    add_field(&mut form);
    form.handle_key(KeyCode::Tab); // → type slot (Text)
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // cursor moves to Username but not applied yet
                                         // Esc closes dropdown — type stays as Text
    let action = form.handle_key(KeyCode::Esc);
    assert_eq!(action, FormAction::None); // did NOT cancel the form
    assert_eq!(form.field_badge(0), "text"); // type unchanged
}
