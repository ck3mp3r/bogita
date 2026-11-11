type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::Tui;
use crate::context::TuiContext;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::{make_app, make_app_with_entry};

#[tokio::test]
async fn esc_dirty_edit_form_then_s_saves() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    let entry_id = tui.main_view.visible_entries()[0].id;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    for c in "x".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }

    // -- Exec: dirty Esc reaches the save/discard modal, then [s] saves
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::ConfirmSave,
        "dirty Esc on an edit form must offer save and discard"
    );
    tui.handle_key_with_modifiers(KeyCode::Char('s'), KeyModifiers::NONE);
    tui.flush_pending().await?;

    // -- Check
    let vaults = tui.app.registry.list_vaults().await?;
    let vault = vaults.first().ok_or("should have a vault")?;
    let identity = tui
        .app
        .identity
        .as_ref()
        .ok_or("should have an identity")?
        .clone();
    let svc = tui.app.registry.vault_service_for(vault, identity);
    let reloaded = svc
        .get_entry(entry_id)
        .await?
        .ok_or("should have reloaded the entry")?;
    assert_eq!(
        reloaded.name, "GitHubx",
        "[s] after a dirty Esc must persist the edit"
    );
    Ok(())
}

#[tokio::test]
async fn b_in_save_modal_returns_to_form() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    for c in "x".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);

    tui.handle_key_with_modifiers(KeyCode::Char('b'), KeyModifiers::NONE);

    let hint = tui.status_hint();
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::FormName,
        "[b] in the save modal must return to the form with edits intact, footer was: {hint}"
    );
    Ok(())
}

#[tokio::test]
async fn esc_dirty_add_form_still_shows_discard_modal() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    for c in "Draft".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }

    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);

    let hint = tui.status_hint();
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::ConfirmDiscard,
        "a dirty add form must keep the discard-only warning, footer was: {hint}"
    );
    Ok(())
}

#[tokio::test]
async fn d_in_add_discard_modal_discards() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    for c in "Draft".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);

    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE);

    let hint = tui.status_hint();
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::MainEntries,
        "[d] in the add-mode discard modal must return to the main view, footer was: {hint}"
    );
    Ok(())
}
