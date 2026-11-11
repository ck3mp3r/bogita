//!
//! The lock screen paints the passphrase inside its own input box.
//!
//! The box was one row tall, so a bordered widget could only paint its top
//! edge and every mask glyph was clipped away. These pins drive the live path
//! (draw, key, draw) and read the painted buffer: the dots must sit inside the
//! input rect, and nothing may paint outside the modal.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::Tui;
use crate::context::TuiContext;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::Terminal;

use super::support::{make_app, TestService};

type TestTui = Tui<TestService>;

const WIDTH: u16 = 80;
const HEIGHT: u16 = 24;
/// Modal size the lock screen asks for.
const MODAL_W: u16 = 50;
const MODAL_H: u16 = 7;

// ── fixtures ────────────────────────────────────────────────────────────────

/// A TUI showing the lock screen.
async fn locked_tui() -> Result<(TestTui, Terminal<TestBackend>)> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);
    let terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT))?;
    Ok((tui, terminal))
}

/// Deliver one key the way the loop does: draw, handle, drain, draw again.
async fn live_key(
    tui: &mut TestTui,
    terminal: &mut Terminal<TestBackend>,
    key: KeyCode,
    modifiers: KeyModifiers,
) -> Result<()> {
    terminal.draw(|frame| tui.render(frame))?;
    tui.handle_key_with_modifiers(key, modifiers);
    tui.flush_pending().await?;
    terminal.draw(|frame| tui.render(frame))?;
    Ok(())
}

/// The modal the lock screen paints: 50x7 centered in the body area, which is
/// the frame minus the one-row header and the one-row status bar.
fn modal_rect() -> Rect {
    let body = Rect::new(0, 1, WIDTH, HEIGHT - 2);
    let x = body.x + body.width.saturating_sub(MODAL_W) / 2;
    let y = body.y + body.height.saturating_sub(MODAL_H) / 2;
    Rect::new(x, y, MODAL_W, MODAL_H)
}

/// The symbol at (x, y), empty when the cell is blank.
fn symbol_at(terminal: &Terminal<TestBackend>, x: u16, y: u16) -> String {
    terminal
        .backend()
        .buffer()
        .cell((x, y))
        .map(|cell| cell.symbol().to_string())
        .unwrap_or_default()
}

/// The text of one painted row, trimmed.
fn row_text(terminal: &Terminal<TestBackend>, y: u16) -> String {
    let mut text = String::new();
    for x in 0..WIDTH {
        text.push_str(&symbol_at(terminal, x, y));
    }
    text.trim().to_string()
}

/// Every painted cell of the input's content row, as (x, glyph).
fn input_row(terminal: &Terminal<TestBackend>, modal: Rect) -> Vec<(u16, String)> {
    (modal.x + 2..modal.x + MODAL_W - 2)
        .map(|x| (x, symbol_at(terminal, x, modal.y + 3)))
        .filter(|(_, glyph)| glyph != " ")
        .collect()
}

// ── the painted box ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_lock_render_typing_shows_masked_dots_inside_the_box() -> Result<()> {
    // -- Setup & Fixtures
    let (mut tui, mut terminal) = locked_tui().await?;
    terminal.draw(|frame| tui.render(frame))?;
    let modal = modal_rect();

    // -- Exec: five strokes, each with a frame in between
    for c in "abcde".chars() {
        live_key(
            &mut tui,
            &mut terminal,
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )
        .await?;
    }

    // -- Check: one masked glyph per stroke, on the input's content row
    let cells = input_row(&terminal, modal);
    assert_eq!(
        cells.len(),
        5,
        "five strokes must paint five glyphs inside the box, got {cells:?}"
    );

    // -- Check: the mask repeats one glyph and never echoes the plaintext
    let glyph = cells
        .first()
        .map(|(_, glyph)| glyph.clone())
        .unwrap_or_default();
    for (x, seen) in &cells {
        assert_eq!(
            seen, &glyph,
            "the mask must repeat one glyph, got ({x},{seen})"
        );
    }
    for plain in "abcde".chars() {
        assert!(
            !cells.iter().any(|(_, seen)| seen.contains(plain)),
            "the input must never echo {plain:?}, got {cells:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn test_lock_render_no_dots_outside_the_modal() -> Result<()> {
    // -- Setup & Fixtures
    let (mut tui, mut terminal) = locked_tui().await?;
    terminal.draw(|frame| tui.render(frame))?;
    let modal = modal_rect();

    // -- Exec
    for c in "secretpass".chars() {
        live_key(
            &mut tui,
            &mut terminal,
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )
        .await?;
    }

    // -- Check: the mask glyph is the one the box paints, and the box got all ten
    let glyph = input_row(&terminal, modal)
        .first()
        .map(|(_, glyph)| glyph.clone())
        .ok_or("the input must paint its mask")?;
    assert_eq!(input_row(&terminal, modal).len(), 10);

    // -- Check: nothing masked paints outside the dialog
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let inside =
                x > modal.x && x < modal.x + MODAL_W && y > modal.y && y < modal.y + MODAL_H;
            let symbol = symbol_at(&terminal, x, y);
            assert!(
                inside || !symbol.contains(&glyph),
                "a mask glyph painted outside the modal at ({x},{y})"
            );
        }
    }

    // -- Check: the header owns its row
    assert_eq!(row_text(&terminal, 0), tui.header_text());
    Ok(())
}

#[tokio::test]
async fn test_lock_render_input_box_is_three_rows() -> Result<()> {
    // -- Setup & Fixtures
    let (mut tui, mut terminal) = locked_tui().await?;

    // -- Exec
    terminal.draw(|frame| tui.render(frame))?;

    // -- Check: the prompt row sits above the box
    let modal = modal_rect();
    assert!(
        row_text(&terminal, modal.y + 1).contains("Enter passphrase:"),
        "the prompt must own its own row, got {}",
        row_text(&terminal, modal.y + 1)
    );

    // -- Check: the box is three rows: top border, content, bottom border
    assert_eq!(symbol_at(&terminal, modal.x + 1, modal.y + 2), "┌");
    assert_eq!(
        symbol_at(&terminal, modal.x + MODAL_W - 2, modal.y + 2),
        "┐"
    );
    assert_eq!(symbol_at(&terminal, modal.x + 1, modal.y + 4), "└");
    assert_eq!(
        symbol_at(&terminal, modal.x + MODAL_W - 2, modal.y + 4),
        "┘"
    );

    // -- Check: one padding row, then the modal's bottom border, no dead rows
    assert_eq!(symbol_at(&terminal, modal.x, modal.y + MODAL_H - 1), "╚");
    Ok(())
}

#[tokio::test]
async fn test_lock_render_error_replaces_the_prompt() -> Result<()> {
    // -- Setup & Fixtures
    let (mut tui, mut terminal) = locked_tui().await?;
    terminal.draw(|frame| tui.render(frame))?;
    let modal = modal_rect();

    // -- Exec: a wrong passphrase, submitted
    for c in "wrong".chars() {
        live_key(
            &mut tui,
            &mut terminal,
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )
        .await?;
    }
    live_key(&mut tui, &mut terminal, KeyCode::Enter, KeyModifiers::NONE).await?;

    // -- Check: the modal reports the failure in the prompt row
    let row = row_text(&terminal, modal.y + 1);
    assert!(
        !row.contains("Enter passphrase:"),
        "a failed unlock must replace the prompt, got {row}"
    );
    assert!(
        !row.trim_matches(['║', ' ']).is_empty(),
        "the prompt row must show the error, got {row}"
    );

    // -- Check: the box emptied
    assert_eq!(
        input_row(&terminal, modal).len(),
        0,
        "a failed unlock clears the passphrase"
    );
    Ok(())
}
