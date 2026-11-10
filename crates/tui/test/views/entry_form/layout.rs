use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Terminal;

use crate::views::entry_form::EntryForm;

use super::support::*;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

const WIDTH: u16 = 100;
const HEIGHT: u16 = 14;

/// A form with one field row: key "host", type username, value "alice".
fn form_with_one_row() -> EntryForm {
    let mut form = add_form_named("Test");
    add_field(&mut form);
    for c in "host".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // 1 = username
    form.handle_key(KeyCode::Enter);
    form.handle_key(KeyCode::Tab); // → value slot
    for c in "alice".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form
}

/// Render the form and return the row that carries the three cell titles.
fn field_row_titles(form: &mut EntryForm) -> Result<String> {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT))?;
    terminal.draw(|frame| form.render(frame, Rect::new(0, 0, WIDTH, HEIGHT)))?;
    let buffer = terminal.backend().buffer();
    for y in 0..buffer.area.height {
        let mut line = String::new();
        for x in 0..buffer.area.width {
            line.push_str(buffer[(x, y)].symbol());
        }
        if line.contains(" Key ") && line.contains(" Type ") && line.contains(" Value ") {
            return Ok(line);
        }
    }
    Err("the field row titles were not rendered on one line".into())
}

#[test]
fn field_row_renders_key_then_type_then_value() -> Result<()> {
    // -- Setup & Fixtures
    let mut form = form_with_one_row();

    // -- Exec
    let line = field_row_titles(&mut form)?;

    // -- Check
    let key = line.find(" Key ").ok_or("missing Key cell")?;
    let type_cell = line.find(" Type ").ok_or("missing Type cell")?;
    let value = line.find(" Value ").ok_or("missing Value cell")?;
    assert!(
        key < type_cell && type_cell < value,
        "columns must read Key | Type | Value, got: {line}"
    );
    Ok(())
}
