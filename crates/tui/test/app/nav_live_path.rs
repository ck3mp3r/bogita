//!
//! Live-path navigation: `g`/`G` delivered the way the event loop does it —
//! draw, then read, then handle, then drain.
//!
//! The view tests press `g` twice with no frame in between. The live loop always
//! draws between two keys, so these cases prove (a) the pending-`g` flag survives
//! a render cycle and (b) both modifier shapes a terminal may report for a
//! capital `G` reach the bound jump.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::Tui;
use crate::context::TuiContext;
use bogita_core::app::App;
use bogita_core::ports::{EncryptedIdentityFile, SqliteStorageFactory};
use bogita_core::test_support::{test_entry, MockKeychain};
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;

use super::support::make_app;

type TestApp = App<MockKeychain, SqliteStorageFactory, EncryptedIdentityFile>;
type TestTui = Tui<MockKeychain, SqliteStorageFactory, EncryptedIdentityFile>;

// ── fixtures ─────────────────────────────────────────────────────────────────

/// An app whose Personal vault holds `count` entries, named `entry-0`..
async fn make_app_with_entries(count: usize) -> Result<TestApp> {
    let app = make_app().await?;
    let vaults = app.registry.list_vaults().await?;
    let vault = vaults.first().ok_or("should have a vault")?;
    let identity = app
        .identity
        .as_ref()
        .ok_or("should have an identity")?
        .clone();
    let svc = app.registry.vault_service_for(vault, identity);
    for i in 0..count {
        let entry = test_entry(vault.id, &format!("entry-{i}"), vec![]);
        svc.save_entry(&entry).await?;
    }
    Ok(app)
}

/// One iteration of the live loop: draw, deliver the key, drain the actions.
async fn live_key(
    tui: &mut TestTui,
    terminal: &mut Terminal<TestBackend>,
    key: KeyCode,
    modifiers: KeyModifiers,
) -> Result<()> {
    terminal.draw(|frame| tui.render(frame))?;
    tui.handle_key_with_modifiers(key, modifiers);
    tui.flush_pending().await?;
    Ok(())
}

/// The name of the selected entry, if any.
fn selected_name(tui: &TestTui) -> Option<String> {
    tui.main_view.selected_entry().map(|e| e.name.clone())
}

// ── the delivery shapes ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_live_path_g_and_capital_g_reach_the_bounds() -> Result<()> {
    // -- Setup & Fixtures: three entries, first selected, a full frame between keys
    let app = make_app_with_entries(3).await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
    let first = selected_name(&tui).ok_or("should have a selected entry")?;
    assert_eq!(first, "entry-0");

    // -- Exec: walk to the last row, then `g` → RENDER → `g`
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('j'),
        KeyModifiers::NONE,
    )
    .await?;
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('j'),
        KeyModifiers::NONE,
    )
    .await?;
    assert_eq!(selected_name(&tui).as_deref(), Some("entry-2"));
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('g'),
        KeyModifiers::NONE,
    )
    .await?;
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('g'),
        KeyModifiers::NONE,
    )
    .await?;

    // -- Check: the pending `g` survived the frame between the two presses
    assert_eq!(
        selected_name(&tui).as_deref(),
        Some("entry-0"),
        "gg must reach the first row across a render cycle"
    );

    // -- Exec: legacy capital G, no modifiers on the event
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('G'),
        KeyModifiers::NONE,
    )
    .await?;

    // -- Check
    assert_eq!(
        selected_name(&tui).as_deref(),
        Some("entry-2"),
        "legacy G must reach the last row"
    );

    // -- Exec: capital G with SHIFT on the event (the CSI-u / kitty shape)
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('g'),
        KeyModifiers::NONE,
    )
    .await?;
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('g'),
        KeyModifiers::NONE,
    )
    .await?;
    assert_eq!(selected_name(&tui).as_deref(), Some("entry-0"));
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('G'),
        KeyModifiers::SHIFT,
    )
    .await?;

    // -- Check
    assert_eq!(
        selected_name(&tui).as_deref(),
        Some("entry-2"),
        "SHIFT+G must reach the last row"
    );
    Ok(())
}

#[tokio::test]
async fn test_live_path_shift_lowercase_g_reaches_the_last_row() -> Result<()> {
    // -- Setup & Fixtures: the other shape a terminal may use for capital G —
    // the lowercase code point plus SHIFT
    let app = make_app_with_entries(3).await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
    assert_eq!(selected_name(&tui).as_deref(), Some("entry-0"));

    // -- Exec
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('g'),
        KeyModifiers::SHIFT,
    )
    .await?;

    // -- Check: SHIFT+`g` is the capital chord, not the first `g` of `gg`
    assert_eq!(
        selected_name(&tui).as_deref(),
        Some("entry-2"),
        "SHIFT+g must reach the last row"
    );
    Ok(())
}

// ── the help overlay owns the whole grammar while it is open ────────────────

/// Open the overlay and paint it, so its scroll bounds exist.
async fn open_help(tui: &mut TestTui, terminal: &mut Terminal<TestBackend>) -> Result<u16> {
    live_key(tui, terminal, KeyCode::Char('?'), KeyModifiers::SHIFT).await?;
    // The loop paints at the top of every iteration, so the first key after the
    // toggle is read against a frame that already carries the overlay bounds.
    terminal.draw(|frame| tui.render(frame))?;
    assert_eq!(tui.help, Some(0), "the overlay opens at the top");
    assert!(
        tui.help_max_scroll > 5,
        "the overlay must overflow an 80x24 frame, got {}",
        tui.help_max_scroll
    );
    Ok(tui.help_max_scroll)
}

#[tokio::test]
async fn test_live_path_help_reaches_both_ends() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
    let max = open_help(&mut tui, &mut terminal).await?;

    // -- Exec: walk down three rows, then `g` → RENDER → `g`
    for _ in 0..3 {
        live_key(
            &mut tui,
            &mut terminal,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        )
        .await?;
    }
    assert_eq!(tui.help, Some(3), "j steps one row");
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('g'),
        KeyModifiers::NONE,
    )
    .await?;
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('g'),
        KeyModifiers::NONE,
    )
    .await?;

    // -- Check: the pending g survived the frame between the presses
    assert_eq!(tui.help, Some(0), "gg must reach the top");

    // -- Exec: the capital chord as the uppercase glyph
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('G'),
        KeyModifiers::NONE,
    )
    .await?;

    // -- Check
    assert_eq!(tui.help, Some(max), "G must reach the end");

    // -- Exec: one bare g after G arms a new sequence instead of re-jumping
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('g'),
        KeyModifiers::NONE,
    )
    .await?;

    // -- Check: G left no pending g behind
    assert_eq!(tui.help, Some(max), "G must not leave a pending g behind");

    // -- Exec: the second g of that sequence
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('g'),
        KeyModifiers::NONE,
    )
    .await?;

    // -- Check
    assert_eq!(tui.help, Some(0), "a fresh gg still reaches the top");
    Ok(())
}

#[tokio::test]
async fn test_live_path_help_shift_g_reaches_the_end() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
    let max = open_help(&mut tui, &mut terminal).await?;

    // -- Exec: the CSI-u shape — the base code point plus SHIFT
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('g'),
        KeyModifiers::SHIFT,
    )
    .await?;

    // -- Check
    assert_eq!(tui.help, Some(max), "SHIFT+g must reach the end");

    // -- Exec: SHIFT must not arm a `gg` sequence
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('g'),
        KeyModifiers::NONE,
    )
    .await?;

    // -- Check
    assert_eq!(
        tui.help,
        Some(max),
        "SHIFT+g must not leave a pending g behind"
    );

    // -- Exec: the uppercase glyph with SHIFT set as well
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('G'),
        KeyModifiers::SHIFT,
    )
    .await?;

    // -- Check
    assert_eq!(tui.help, Some(max), "SHIFT+G must reach the end too");
    Ok(())
}

#[tokio::test]
async fn test_live_path_help_half_page_and_disarm() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
    let max = open_help(&mut tui, &mut terminal).await?;
    let half = (tui.help_viewport / 2).max(1);

    // -- Exec: one half page down
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('d'),
        KeyModifiers::CONTROL,
    )
    .await?;

    // -- Check
    assert_eq!(
        tui.help,
        Some(half),
        "Ctrl-d moves half the viewport of {}",
        tui.help_viewport
    );

    // -- Exec: half pages past the end clamp
    for _ in 0..40 {
        live_key(
            &mut tui,
            &mut terminal,
            KeyCode::Char('d'),
            KeyModifiers::CONTROL,
        )
        .await?;
    }

    // -- Check
    assert_eq!(tui.help, Some(max), "half pages clamp at the end");

    // -- Exec: one half page back up
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('u'),
        KeyModifiers::CONTROL,
    )
    .await?;

    // -- Check
    assert_eq!(tui.help, Some(max - half));

    // -- Exec: a stray key between the two g presses clears the pending g
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('g'),
        KeyModifiers::NONE,
    )
    .await?;
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('j'),
        KeyModifiers::NONE,
    )
    .await?;
    live_key(
        &mut tui,
        &mut terminal,
        KeyCode::Char('g'),
        KeyModifiers::NONE,
    )
    .await?;

    // -- Check: the stray j stepped one row instead of the pair jumping
    assert_eq!(
        tui.help,
        Some(max - half + 1),
        "a stray key must clear the pending g"
    );
    Ok(())
}
