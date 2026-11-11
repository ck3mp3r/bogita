//!
//! Dialogs: the save modal is gated on actual changes, and every dialog shows
//! its own action row in-box (sourced from the same tables the overlay lists).

use crate::app::bindings;
use crate::app::Tui;
use crate::context::TuiContext;
use bogita_core::ports::{EncryptedIdentityFile, SqliteStorageFactory};
use bogita_core::test_support::MockKeychain;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;

use super::support::{make_app, make_app_with_entry};

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;
type TestTui = Tui<MockKeychain, SqliteStorageFactory, EncryptedIdentityFile>;

const NO_CHANGES: &str = "No changes to save";

/// Press one unmodified key.
fn key(tui: &mut TestTui, k: KeyCode) {
    tui.handle_key_with_modifiers(k, KeyModifiers::NONE);
}

/// Draw the whole app and flatten the screen.
fn rendered(tui: &mut TestTui) -> Result<String> {
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| tui.render(frame))?;
    Ok(terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect())
}

/// The transient status line, if one is showing.
fn status(tui: &TestTui) -> String {
    tui.status_message
        .as_ref()
        .map(|(message, _)| message.clone())
        .unwrap_or_default()
}

// ── the clean edit form does not ask ────────────────────────────────────────

#[tokio::test]
async fn test_clean_edit_enter_closes_without_prompt() -> Result<()> {
    // -- Setup & Fixtures: open the editor and change nothing
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    key(&mut tui, KeyCode::Char(' '));
    key(&mut tui, KeyCode::Char('e'));

    // -- Exec
    key(&mut tui, KeyCode::Enter);

    // -- Check: no modal, back to Main, and the user is told why
    assert_eq!(
        tui.surface(),
        bindings::Surface::MainEntries,
        "a clean edit form must close instead of asking about changes"
    );
    assert_eq!(status(&tui), NO_CHANGES);
    assert!(tui.pending.is_none(), "a clean form must queue no write");
    Ok(())
}

#[tokio::test]
async fn test_clean_edit_esc_closes_without_prompt() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    key(&mut tui, KeyCode::Char(' '));
    key(&mut tui, KeyCode::Char('e'));

    // -- Exec
    key(&mut tui, KeyCode::Esc);

    // -- Check
    assert_eq!(tui.surface(), bindings::Surface::MainEntries);
    assert_eq!(status(&tui), NO_CHANGES);
    Ok(())
}

#[tokio::test]
async fn test_dirty_edit_enter_still_prompts() -> Result<()> {
    // -- Setup & Fixtures: type into the name so the form is dirty
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    key(&mut tui, KeyCode::Char(' '));
    key(&mut tui, KeyCode::Char('e'));
    key(&mut tui, KeyCode::Char('x'));

    // -- Exec
    key(&mut tui, KeyCode::Enter);

    // -- Check: the modal is the thing that asks about changes
    assert_eq!(tui.surface(), bindings::Surface::ConfirmSave);
    Ok(())
}

// ── dialogs show their own keys ─────────────────────────────────────────────

#[tokio::test]
async fn test_dialog_shows_in_box_keys() -> Result<()> {
    // -- Setup & Fixtures: a dirty edit form, then the save modal
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    key(&mut tui, KeyCode::Char(' '));
    key(&mut tui, KeyCode::Char('e'));
    key(&mut tui, KeyCode::Char('x'));
    key(&mut tui, KeyCode::Enter);
    assert_eq!(tui.surface(), bindings::Surface::ConfirmSave);

    // -- Exec
    let screen = rendered(&mut tui)?;

    // -- Check: the in-box row is the surface's own table, rendered inside the box
    let expected = bindings::hint(bindings::table(bindings::Surface::ConfirmSave));
    assert!(
        screen.contains(&expected),
        "the save dialog must show its action row `{expected}`, rendered: {screen}"
    );
    Ok(())
}

#[tokio::test]
async fn test_every_dialog_renders_its_table() -> Result<()> {
    // -- Setup & Fixtures: the discard modal, reached from a dirty add form
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    key(&mut tui, KeyCode::Char(' '));
    key(&mut tui, KeyCode::Char('a'));
    key(&mut tui, KeyCode::Char('D'));
    key(&mut tui, KeyCode::Esc);
    assert_eq!(tui.surface(), bindings::Surface::ConfirmDiscard);

    // -- Exec
    let screen = rendered(&mut tui)?;

    // -- Check
    let expected = bindings::hint(bindings::table(bindings::Surface::ConfirmDiscard));
    assert!(
        screen.contains(&expected),
        "the discard dialog must show its action row `{expected}`, rendered: {screen}"
    );

    // -- Exec: the delete dialog
    key(&mut tui, KeyCode::Char('d')); // discard the draft, back to Main
    assert_eq!(tui.surface(), bindings::Surface::MainEntries);
    key(&mut tui, KeyCode::Char(' '));
    key(&mut tui, KeyCode::Char('d'));
    let screen = rendered(&mut tui)?;

    // -- Check
    let expected = bindings::hint(bindings::table(bindings::Surface::ConfirmDelete));
    assert!(
        screen.contains(&expected),
        "the delete dialog must show its action row `{expected}`, rendered: {screen}"
    );
    Ok(())
}

#[tokio::test]
async fn test_error_dialog_shows_its_dismiss_row() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.error_message = Some("vault locked".to_string());
    assert_eq!(tui.surface(), bindings::Surface::ErrorOverlay);

    // -- Exec
    let screen = rendered(&mut tui)?;

    // -- Check
    let expected = bindings::hint(bindings::table(bindings::Surface::ErrorOverlay));
    assert!(
        screen.contains(&expected),
        "the error dialog must show `{expected}`, rendered: {screen}"
    );
    Ok(())
}
