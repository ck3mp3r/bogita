//! Structural checks for the composed field row: three cells, one context, and
//! the anchor the type popup rides on.

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

/// Render the form and return the line that carries the three cell titles.
fn render_field_row(form: &mut EntryForm) -> Result<String> {
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
fn test_row_widget_renders_three_cells() -> Result<()> {
    // -- Setup & Fixtures
    let mut form = form_with_one_row();

    // -- Exec
    let line = render_field_row(&mut form)?;

    // -- Check
    let key = line.find(" Key ").ok_or("missing Key cell")?;
    let type_cell = line.find(" Type ").ok_or("missing Type cell")?;
    let value = line.find(" Value ").ok_or("missing Value cell")?;
    assert!(
        key < type_cell && type_cell < value,
        "the row must paint Key, then Type, then Value, got: {line}"
    );
    // 35% / 15% / 50% of the inner width: the value cell owns the last half.
    let inner_mid = usize::from(WIDTH) / 2;
    assert!(
        value >= inner_mid,
        "the value cell must start in the right half of the row, got {value} in: {line}"
    );
    Ok(())
}

#[test]
fn test_row_ctx_reports_the_same_errors_as_validation() -> Result<()> {
    // -- Setup & Fixtures: an untouched key makes the key cell invalid.
    let mut form = add_form_named("Test");
    add_field(&mut form);
    form.handle_key(KeyCode::Char('a')); // touch the key
    form.handle_key(KeyCode::Backspace); // then empty it

    // -- Exec
    let line = render_field_row(&mut form)?;

    // -- Check: the marker rides on the key cell only.
    assert!(
        line.contains(" Key ⚠ "),
        "the key cell must carry the warning marker, got: {line}"
    );
    assert!(
        !line.contains(" Value text ⚠ "),
        "the value cell must stay clean, got: {line}"
    );
    Ok(())
}

#[test]
fn test_form_paints_no_key_strip() -> Result<()> {
    // -- Setup & Fixtures
    let mut form = add_form_named("Test");

    // -- Exec
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT))?;
    terminal.draw(|frame| form.render(frame, Rect::new(0, 0, WIDTH, HEIGHT)))?;
    let buffer = terminal.backend().buffer();
    let mut screen = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            screen.push_str(buffer[(x, y)].symbol());
        }
    }

    // -- Check: the keys live behind `?`, not in the modal
    for strip in ["add field", "delete field", "[Alt-n]", "[Alt-d]"] {
        assert!(
            !screen.contains(strip),
            "the form must paint no key strip, found `{strip}`"
        );
    }
    Ok(())
}

#[test]
fn test_type_cell_records_anchor_area() -> Result<()> {
    // -- Setup & Fixtures: the dropdown is open, so the popup pass has work.
    let mut form = add_form_named("Test");
    add_field(&mut form);
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(&mut form);
    assert!(
        form.fields[0].type_state.is_popup_active(),
        "fixture needs the popup open"
    );

    // -- Exec
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT))?;
    terminal.draw(|frame| form.render(frame, Rect::new(0, 0, WIDTH, HEIGHT)))?;

    // -- Check
    let area = form.fields[0].type_state.area;
    assert!(
        area.width > 0 && area.height > 0,
        "the type cell must record the area the popup anchors to, got: {area:?}"
    );
    Ok(())
}
