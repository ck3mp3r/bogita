//!
//! Startup in the locked state: the TUI opens on its own lock screen, so the
//! passphrase is asked inside the TUI and never on the terminal prompt.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::bindings::Surface;
use crate::app::tui::ActiveView;
use crate::app::RunState;
use crate::app::Tui;
use crate::context::TuiContext;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::make_locked_app;

#[tokio::test]
async fn boot_locked_opens_the_lock_screen() -> Result<()> {
    // -- Setup & Fixtures: the app a TUI boot finds when the keychain is empty
    let app = make_locked_app().await?;

    // -- Exec
    let tui = Tui::new(app, TuiContext::Default).await?;

    // -- Check: the TUI itself is the unlock surface
    assert!(
        matches!(tui.active, ActiveView::Locked { .. }),
        "a locked boot must land on the lock screen, not the main view"
    );
    assert_eq!(tui.surface(), Surface::Locked);
    Ok(())
}

#[tokio::test]
async fn boot_locked_routes_typing_to_the_passphrase() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_locked_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // -- Exec
    tui.handle_key_with_modifiers(KeyCode::Char('s'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('c'), KeyModifiers::CONTROL);

    // -- Check: the chars landed in the passphrase, and Ctrl-c still quits
    let ActiveView::Locked {
        passphrase_input, ..
    } = &tui.active
    else {
        panic!("expected the lock screen");
    };
    assert_eq!(passphrase_input.text(), "se");
    assert_eq!(tui.state, RunState::Quit);
    Ok(())
}

#[tokio::test]
async fn boot_locked_esc_quits() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_locked_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // -- Exec
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);

    // -- Check
    assert_eq!(tui.state, RunState::Quit);
    Ok(())
}
