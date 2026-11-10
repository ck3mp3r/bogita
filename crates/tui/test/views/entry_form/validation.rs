use crate::views::entry_form::{EntryForm, SlotKind, ValidationIssue};
use bogita_core::domain::SshKeyMaterial;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

const GARBAGE_PEM: &str =
    "-----BEGIN OPENSSH PRIVATE KEY-----\nnot actually a key\n-----END OPENSSH PRIVATE KEY-----\n";
const OPEN_ARMOR_ONLY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\nabc";

/// Add a field and leave the focus on its value slot; `type_index` picks the
/// dropdown row (3 = totp, 4 = ssh-key) by pressing `j` that many times.
fn form_with_value_slot(type_index: usize) -> EntryForm {
    let mut form = EntryForm::new_add(Some("Deploy key".to_string()));
    form.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT); // → key slot
    for c in "private_key".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
    form.handle_key(KeyCode::Enter); // open dropdown
    for _ in 0..type_index {
        form.handle_key(KeyCode::Char('j'));
    }
    form.handle_key(KeyCode::Enter); // apply
    form.handle_key(KeyCode::Tab); // → value slot
    assert_eq!(
        form.focused_slot(),
        SlotKind::Value(0),
        "fixture must land on the value slot"
    );
    form
}

fn ssh_value_form() -> EntryForm {
    form_with_value_slot(4)
}

fn totp_value_form() -> EntryForm {
    form_with_value_slot(3)
}

#[test]
fn validity_marker_updates_on_edit() {
    // -- Setup & Fixtures
    let mut form = ssh_value_form();

    // -- Exec: a complete but unparsable armor is flagged immediately
    form.handle_paste(GARBAGE_PEM);
    assert!(
        form.validation_errors()
            .contains(&ValidationIssue::InvalidSshKey(0)),
        "complete garbage armor must be flagged"
    );
    assert!(form.validation_error_summary().contains("Field 1"));

    // -- Check: a valid key clears the flag on the next event
    let pem = SshKeyMaterial::keygen_ed25519("realtime@test")
        .expect("keygen an Ed25519 key")
        .private_pem()
        .to_string();
    form.set_focused_value(pem);
    form.handle_key_with_modifiers(KeyCode::Char('r'), KeyModifiers::ALT); // any event re-validates
    assert!(
        !form
            .validation_errors()
            .contains(&ValidationIssue::InvalidSshKey(0)),
        "a valid PEM must clear the flag"
    );
}

#[test]
fn incomplete_armor_not_flagged_while_focused() {
    // -- Setup & Fixtures
    let mut form = ssh_value_form();

    // -- Exec
    form.handle_paste(OPEN_ARMOR_ONLY);

    // -- Check
    assert!(
        !form
            .validation_errors()
            .contains(&ValidationIssue::InvalidSshKey(0)),
        "half a key must not be judged while the slot is focused"
    );
}

#[test]
fn incomplete_armor_flagged_after_leaving_the_slot() {
    // -- Setup & Fixtures
    let mut form = ssh_value_form();
    form.handle_paste(OPEN_ARMOR_ONLY);

    // -- Exec: leave the value slot
    form.handle_key(KeyCode::BackTab);

    // -- Check
    assert!(
        form.validation_errors()
            .contains(&ValidationIssue::InvalidSshKey(0)),
        "a half-typed key must be flagged once the slot is left"
    );
}

#[test]
fn totp_secret_must_be_base32() {
    // -- Setup & Fixtures
    let mut form = totp_value_form();

    // -- Exec
    form.handle_paste("not base32!");

    // -- Check
    assert!(
        form.validation_errors()
            .contains(&ValidationIssue::InvalidTotp(0)),
        "a non-base32 secret must be flagged"
    );
    assert!(form.validation_error_summary().contains("base32"));

    form.set_focused_value("JBSWY3DPEHPK3PXP".to_string());
    form.handle_key_with_modifiers(KeyCode::Char('r'), KeyModifiers::ALT); // any event re-validates
    assert!(
        !form
            .validation_errors()
            .contains(&ValidationIssue::InvalidTotp(0)),
        "a base32 secret must clear the flag"
    );
}

#[test]
fn free_text_rows_are_never_value_flagged() {
    // -- Setup & Fixtures
    let mut form = EntryForm::new_add(Some("Notes entry".to_string()));

    // -- Exec: a Text row with punctuation-heavy content
    form.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT);
    for c in "free".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
    form.handle_key(KeyCode::Tab); // → value slot (Text)
    form.handle_paste("anything!@#\nwith lines");

    // -- Check
    assert!(
        !form.validation_errors().iter().any(|e| matches!(
            e,
            ValidationIssue::InvalidSshKey(_) | ValidationIssue::InvalidTotp(_)
        )),
        "free text stays free"
    );
}

#[test]
fn paste_then_invalid_flags_and_valid_paste_stays_clean() {
    // -- Setup & Fixtures
    let mut form = ssh_value_form();
    let pem = SshKeyMaterial::keygen_ed25519("paste@test")
        .expect("keygen an Ed25519 key")
        .private_pem()
        .to_string();

    // -- Exec
    form.handle_paste(&pem);

    // -- Check
    assert!(
        form.validation_errors().is_empty(),
        "a pasted valid PEM is clean: {:?}",
        form.validation_errors()
    );
}
