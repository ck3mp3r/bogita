use crate::views::entry_form::EntryForm;
use rat_widget::text::HasScreenCursor;
use ratatui::backend::{Backend, TestBackend};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Terminal;

use super::support::*;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

const WIDTH: u16 = 40;
const HEIGHT: u16 = 12;

// -- Helpers

/// Draw `form` on a fresh 40x12 TestBackend and return the terminal caret.
fn render_caret(form: &mut EntryForm) -> Result<(u16, u16)> {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT))?;
    let area = Rect::new(0, 0, WIDTH, HEIGHT);
    terminal.draw(|frame| form.render(frame, area))?;
    let pos = terminal.backend_mut().get_cursor_position()?;
    Ok((pos.x, pos.y))
}

/// A form with one field, focused on the type slot, with the type set to notes.
fn form_on_notes_slot() -> EntryForm {
    let mut form = add_form_named("Test");
    form.handle_key(KeyCode::Char('+')); // add field, focus → key slot
    form.handle_key(KeyCode::Tab); // key → value
    form.handle_key(KeyCode::Tab); // value → type
    open_dropdown(&mut form);
    for _ in 0..5 {
        form.handle_key(KeyCode::Char('j')); // text → notes
    }
    form.handle_key(KeyCode::Enter); // confirm selection
    form.handle_key(KeyCode::BackTab); // type → value (now notes)
    form
}

// ── caret follows the focused slot ───────────────────────────────────────────

#[test]
fn test_entry_form_cursor_name_slot_add_mode() -> Result<()> {
    // -- Setup & Fixtures
    let mut form = add_form();

    // -- Exec
    let caret = render_caret(&mut form)?;
    let expected = form
        .name_state
        .screen_cursor()
        .ok_or("focused name slot should report a screen cursor")?;

    // -- Check
    assert_eq!(
        caret, expected,
        "caret must sit at the focused Name slot text position"
    );
    Ok(())
}

#[test]
fn test_entry_form_cursor_key_slot() -> Result<()> {
    // -- Setup & Fixtures
    let mut form = add_form_named("Test");
    form.handle_key(KeyCode::Char('+')); // add field, focus → key slot

    // -- Exec
    let caret = render_caret(&mut form)?;
    let expected = form.fields[0]
        .key_state
        .screen_cursor()
        .ok_or("focused key slot should report a screen cursor")?;

    // -- Check
    assert_eq!(
        caret, expected,
        "caret must sit at the focused Key slot text position"
    );
    Ok(())
}

#[test]
fn test_entry_form_cursor_plain_value_slot() -> Result<()> {
    // -- Setup & Fixtures
    let mut form = add_form_named("Test");
    form.handle_key(KeyCode::Char('+')); // add field, focus → key slot
    form.handle_key(KeyCode::Tab); // key → value (plain text)

    // -- Exec
    let caret = render_caret(&mut form)?;
    let expected = form.fields[0]
        .value_state
        .screen_cursor()
        .ok_or("focused value slot should report a screen cursor")?;

    // -- Check
    assert_eq!(
        caret, expected,
        "caret must sit at the focused plain Value slot text position"
    );
    Ok(())
}

#[test]
fn test_entry_form_cursor_notes_value_slot() -> Result<()> {
    // -- Setup & Fixtures
    let mut form = form_on_notes_slot();

    // -- Exec
    let caret = render_caret(&mut form)?;
    let textarea = form.fields[0]
        .textarea_state
        .as_ref()
        .ok_or("notes field should own a TextAreaState")?;
    let expected = textarea
        .screen_cursor()
        .ok_or("focused notes slot should report a screen cursor")?;

    // -- Check
    assert_eq!(
        caret, expected,
        "caret must sit at the focused Notes value text position"
    );
    Ok(())
}

#[test]
fn test_entry_form_cursor_edit_mode_name_slot() -> Result<()> {
    // -- Setup & Fixtures
    use bogita_core::domain::{Entry, EntryType, Field, FieldType, FieldValue};
    use chrono::Utc;
    use uuid::Uuid;

    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: "alice@corp".to_string(),
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

    // -- Exec
    let caret = render_caret(&mut form)?;
    let expected = form
        .name_state
        .screen_cursor()
        .ok_or("focused name slot should report a screen cursor")?;

    // -- Check
    assert_eq!(
        caret, expected,
        "caret must sit at the focused Name slot text position in edit mode"
    );
    assert_eq!(
        caret.0,
        form.name_state.inner.x + 10,
        "caret must follow the last typed character of 'alice@corp'"
    );
    Ok(())
}

// ── no caret while no text slot is focused ────────────────────────────────────

#[test]
fn test_entry_form_cursor_type_slot_leaves_caret_untouched() -> Result<()> {
    // -- Setup & Fixtures
    let mut form = add_form_named("Test");
    form.handle_key(KeyCode::Char('+')); // add field, focus → key slot
    form.handle_key(KeyCode::Tab); // key → value
    form.handle_key(KeyCode::Tab); // value → type slot

    // -- Exec
    let caret = render_caret(&mut form)?;

    // -- Check
    assert_eq!(
        caret,
        (0, 0),
        "no text slot is focused, so the form must not move the terminal caret"
    );
    Ok(())
}
