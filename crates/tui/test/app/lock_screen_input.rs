type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::{tui::ActiveView, Tui};
use crate::context::TuiContext;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::*;

// ── lock screen — input gating ────────────────────────────────────────────────

#[tokio::test]
async fn lock_screen_ignores_modified_chars() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    // -- Exec
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('x'), KeyModifiers::ALT);
    tui.handle_key_with_modifiers(KeyCode::Char('y'), KeyModifiers::CONTROL);

    // -- Check
    let ActiveView::Locked {
        passphrase_input, ..
    } = &tui.active
    else {
        panic!("expected the lock screen");
    };
    assert_eq!(passphrase_input.text(), "a");
    Ok(())
}

#[tokio::test]
async fn lock_screen_inserts_shifted_chars() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    // -- Exec
    tui.handle_key_with_modifiers(KeyCode::Char('A'), KeyModifiers::SHIFT);

    // -- Check
    let ActiveView::Locked {
        passphrase_input, ..
    } = &tui.active
    else {
        panic!("expected the lock screen");
    };
    assert_eq!(passphrase_input.text(), "A");
    Ok(())
}
