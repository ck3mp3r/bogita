//!
//! Startup in the locked state: the TUI opens on its own lock screen, so the
//! passphrase is asked inside the TUI and never on the terminal prompt.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::bindings::Surface;
use crate::app::tui::ActiveView;
use crate::app::RunState;
use crate::app::Tui;
use crate::context::TuiContext;
use bogita_core::agent::VaultService;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;

use super::support::{make_locked_app, make_unlockable_app, UNLOCK_PASSPHRASE};

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

#[tokio::test]
async fn boot_locked_unlock_opens_the_main_view() -> Result<()> {
    // -- Setup & Fixtures: a locked boot whose identity file a passphrase opens
    let app = make_unlockable_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    assert!(matches!(tui.active, ActiveView::Locked { .. }));

    // -- Exec: type the passphrase and submit; the flush drains the unlock
    for c in UNLOCK_PASSPHRASE.chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    tui.flush_pending().await?;

    // -- Check: the service is unlocked and the vault is loaded
    assert_eq!(tui.surface(), Surface::MainEntries);
    assert!(
        !tui.service.is_locked(),
        "the passphrase must unlock the service"
    );
    assert_eq!(
        tui.main_view.visible_entries().len(),
        1,
        "the unlock must load the vault entries"
    );
    Ok(())
}

#[tokio::test]
async fn boot_locked_unlock_renders_the_registry() -> Result<()> {
    // -- Setup & Fixtures: a locked boot whose identity file a passphrase opens
    let app = make_unlockable_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // -- Exec: unlock through the lock screen, then paint the frame the loop paints
    for c in UNLOCK_PASSPHRASE.chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    tui.flush_pending().await?;
    let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
    terminal.draw(|frame| tui.render(frame))?;

    // -- Check: the entry the service decrypted is on screen
    let rendered: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(
        rendered.contains("GitHub"),
        "the unlocked registry must render its entries, rendered: {rendered}"
    );
    Ok(())
}

#[tokio::test]
async fn boot_locked_wrong_passphrase_keeps_the_lock_screen() -> Result<()> {
    // -- Setup & Fixtures: the identity file carries a different passphrase
    let app = make_unlockable_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // -- Exec: type and submit a wrong passphrase, then drain the deferred unlock
    for c in "wrong".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    tui.flush_pending().await?;

    // -- Check: the refusal stays on the lock screen and the input is cleared
    let ActiveView::Locked {
        passphrase_input,
        error,
    } = &tui.active
    else {
        panic!("a refused unlock must stay on the lock screen");
    };
    assert!(
        error.is_some(),
        "the refusal must be shown on the lock screen"
    );
    assert_eq!(
        passphrase_input.text(),
        "",
        "the refusal must clear the passphrase"
    );
    assert!(tui.service.is_locked(), "the service must stay locked");
    Ok(())
}
