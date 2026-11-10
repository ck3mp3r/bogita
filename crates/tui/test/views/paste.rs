use crate::views::entry_form::{EntryForm, FormAction};
use crate::views::{normalize_paste, paste_to_single_line};
use bogita_core::domain::{FieldValue, SshKeyMaterial};
use rat_event::{HandleEvent, Regular};
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

// ── helpers ───────────────────────────────────────────────────────────────────

#[test]
fn normalize_paste_turns_crlf_into_lf() {
    assert_eq!(normalize_paste("a\r\nb"), "a\nb");
}

#[test]
fn normalize_paste_turns_lone_cr_into_lf() {
    assert_eq!(normalize_paste("a\rb"), "a\nb");
}

#[test]
fn normalize_paste_keeps_lf_untouched() {
    assert_eq!(normalize_paste("a\nb"), "a\nb");
}

#[test]
fn normalize_paste_handles_mixed_line_breaks() {
    assert_eq!(normalize_paste("a\r\nb\rc\nd"), "a\nb\nc\nd");
}

#[test]
fn paste_to_single_line_replaces_each_break_with_one_space() {
    assert_eq!(paste_to_single_line("a\r\nb\rc\nd"), "a b c d");
}

#[test]
fn paste_to_single_line_keeps_text_without_breaks() {
    assert_eq!(paste_to_single_line("plain"), "plain");
}

// ── widget characterization: rat-text keeps pasted newlines ───────────────────

/// An add form with one SshKey row, focused on its value slot.
fn ssh_value_form() -> EntryForm {
    let mut form = EntryForm::new_add(Some("Deploy key".to_string()));
    form.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT); // → key slot
    for c in "private_key".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
    form.handle_key(KeyCode::Enter); // open dropdown
    for _ in 0..4 {
        form.handle_key(KeyCode::Char('j')); // → ssh-key
    }
    form.handle_key(KeyCode::Enter); // apply
    form.handle_key(KeyCode::Tab); // → value slot
    form
}

#[test]
fn widget_paste_keeps_newlines_in_an_ssh_value() -> Result<()> {
    let mut form = ssh_value_form();
    let pem = SshKeyMaterial::keygen_ed25519("test")?
        .private_pem()
        .to_string();
    let event = Event::Paste(pem.clone());

    let _ = form.handle(&event, Regular);

    let FormAction::Confirm(entry) = form.handle_key(KeyCode::Enter) else {
        panic!("the form should confirm");
    };
    assert!(
        matches!(&entry.fields[0].value, FieldValue::Hidden(v) if v == &pem),
        "the widget layer must keep pasted newlines, got: {:?}",
        entry.fields[0].value
    );
    Ok(())
}

#[test]
fn type_slot_ignores_a_paste() {
    let mut form = EntryForm::new_add(Some("Deploy key".to_string()));
    form.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT); // → key slot
    form.handle_key(KeyCode::Tab); // → type slot

    let _ = form.handle(&Event::Paste("text".to_string()), Regular);

    assert_eq!(
        form.field_badge(0),
        "text",
        "a paste must not retype the field"
    );
}
