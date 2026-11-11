type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::Tui;
use crate::context::TuiContext;
use bogita_core::agent::VaultService;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::{make_app, make_app_with_entry};

#[tokio::test]
async fn edit_enter_on_save_modal_saves() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    let entry_id = tui.main_view.visible_entries()[0].id;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    for c in "x".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }

    // -- Exec: Enter opens the save modal, a second Enter completes the save
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::ConfirmSave,
        "Enter on the edit form should open the save modal"
    );
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    tui.flush_pending().await?;

    // -- Check
    let reloaded = tui
        .service
        .get_entry(entry_id)
        .await?
        .ok_or("should have reloaded the entry")?;
    assert_eq!(
        reloaded.name, "GitHubx",
        "Enter on the save modal must persist the edit"
    );
    Ok(())
}

#[tokio::test]
async fn enter_on_discard_modal_returns_to_form() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    for c in "Draft".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);

    // -- Exec: Enter on the discard modal goes back to the form
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);

    // -- Check
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::FormName,
        "Enter on the discard modal must return to the form with the edits intact"
    );
    Ok(())
}

#[tokio::test]
async fn enter_on_delete_modal_keeps_the_modal() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE);

    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::ConfirmDelete,
        "Enter must not confirm a destructive delete"
    );
    Ok(())
}

#[tokio::test]
async fn enter_on_add_text_slot_still_saves_immediately() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    for c in "Bravo".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }

    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    tui.flush_pending().await?;

    assert_eq!(
        tui.main_view.selected_entry().map(|e| e.name.as_str()),
        Some("Bravo"),
        "Enter on an add-form text slot must keep saving immediately"
    );
    Ok(())
}
