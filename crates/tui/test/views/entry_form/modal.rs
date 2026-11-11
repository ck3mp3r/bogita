//! Modal contract: identity in the title, single-border children, no dead rows.

use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Terminal;

use crate::views::entry_form::EntryForm;
use bogita_core::domain::{Entry, EntryType, FieldValue};
use bogita_core::test_support::test_field;
use chrono::Utc;
use uuid::Uuid;

use super::support::*;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

const WIDTH: u16 = 80;
const HEIGHT: u16 = 24;

/// An edit form on a named entry with one token field.
fn edit_form_named(name: &str) -> EntryForm {
    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: name.to_string(),
        entry_type: EntryType::Token,
        fields: vec![test_field(
            "password",
            FieldValue::Hidden("s3cret".to_string()),
            bogita_core::domain::FieldType::Token,
            true,
            0,
        )],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    };
    EntryForm::new_edit(&entry)
}

/// Render the form into a fresh buffer and return the whole screen as one string
/// plus the per-row strings.
fn render_lines(form: &mut EntryForm) -> Result<Vec<String>> {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT))?;
    terminal.draw(|frame| form.render(frame, Rect::new(0, 0, WIDTH, HEIGHT)))?;
    let buffer = terminal.backend().buffer();
    let mut lines = Vec::new();
    for y in 0..buffer.area.height {
        let mut line = String::new();
        for x in 0..buffer.area.width {
            line.push_str(buffer[(x, y)].symbol());
        }
        lines.push(line);
    }
    Ok(lines)
}

#[test]
fn test_modal_title_carries_entry_name() -> Result<()> {
    // -- Setup & Fixtures: dirty the form so the mark is part of the title.
    let mut form = edit_form_named("GitHub");
    form.handle_key(KeyCode::Char('x'));

    // -- Exec
    let lines = render_lines(&mut form)?;
    let screen = lines.join("\n");

    // -- Check
    assert!(
        screen.contains(" Edit · GitHubx * "),
        "the edit modal must carry the entry name and the dirty mark, rendered: {screen}"
    );
    Ok(())
}

#[test]
fn test_new_modal_names_the_vault() -> Result<()> {
    // -- Setup & Fixtures
    let mut form = add_form_named("");
    let vault_id = Uuid::new_v4();
    form.set_vault(vault_id, "Personal".to_string());

    // -- Exec
    let lines = render_lines(&mut form)?;
    let screen = lines.join("\n");

    // -- Check
    assert!(
        screen.contains(" New · Personal "),
        "the add modal must name the vault, rendered: {screen}"
    );
    Ok(())
}

#[test]
fn test_no_double_borders_inside_modal() -> Result<()> {
    // -- Setup & Fixtures
    let mut form = edit_form_named("GitHub");

    // -- Exec
    let lines = render_lines(&mut form)?;

    // -- Check: only the modal frame may draw double corners.
    for (y, line) in lines.iter().enumerate() {
        let is_frame_row = y == 0 || y + 1 == lines.len();
        if is_frame_row {
            continue;
        }
        for glyph in ["╔", "╗", "╚", "╝"] {
            assert!(
                !line.contains(glyph),
                "row {y} must not nest a double border inside the modal: {line}"
            );
        }
    }
    Ok(())
}
