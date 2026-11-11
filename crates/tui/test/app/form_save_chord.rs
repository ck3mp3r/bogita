type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::Tui;
use crate::context::TuiContext;
use bogita_core::agent::VaultService;
use bogita_core::domain::FieldValue;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::{make_app, make_app_with_entry};

#[tokio::test]
async fn alt_s_in_edit_form_shows_save_modal_and_saves() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    let entry_id = tui.main_view.visible_entries()[0].id;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);

    // -- Exec: append to the value, hop back to the type slot, then Alt-s
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → key slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → type slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → value slot
    for c in "xyz".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::BackTab, KeyModifiers::NONE); // → type slot
    tui.handle_key_with_modifiers(KeyCode::Char('s'), KeyModifiers::ALT);

    // -- Check: the save modal opens, and [s] persists the edit
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::ConfirmSave,
        "Alt-s from the type slot should open the save modal"
    );
    tui.handle_key_with_modifiers(KeyCode::Char('s'), KeyModifiers::NONE);
    tui.flush_pending().await?;

    let reloaded = tui
        .service
        .get_entry(entry_id)
        .await?
        .ok_or("should have reloaded the entry")?;
    let password = reloaded
        .fields
        .iter()
        .find(|field| field.key == "password")
        .ok_or("the reloaded entry should keep its password row")?;
    assert!(
        matches!(&password.value, FieldValue::Hidden(v) if v == "s3cretxyz"),
        "the Alt-s save must persist the edited value, got: {:?}",
        password.value
    );
    Ok(())
}

#[tokio::test]
async fn status_hint_on_key_slot_advertises_enter_save() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT); // → key slot

    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::FormKey,
        "the key slot owns the keys"
    );
    assert!(
        crate::app::bindings::hint(crate::app::bindings::FORM_KEY).contains("[Enter] save"),
        "the key slot's inventory must advertise save"
    );
    Ok(())
}

#[tokio::test]
async fn status_hint_on_value_slot_advertises_enter_save() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT); // → key slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → type slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → value slot

    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::FormValue,
        "the value slot owns the keys"
    );
    assert!(
        crate::app::bindings::hint(crate::app::bindings::FORM_VALUE).contains("[Enter] save"),
        "the value slot's inventory must advertise save"
    );
    Ok(())
}

#[tokio::test]
async fn status_hint_on_token_slot_advertises_enter_save() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT); // → key slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → type slot
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // open dropdown
    tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // Username
    tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // Token
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // apply
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → token slot

    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::FormToken,
        "the token slot owns the keys"
    );
    assert!(
        crate::app::bindings::hint(crate::app::bindings::FORM_TOKEN).contains("[Enter] save"),
        "the token slot's inventory must advertise save"
    );
    Ok(())
}

#[tokio::test]
async fn status_hint_on_type_slot_advertises_alt_s_save() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT); // → key slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → type slot

    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::FormType,
        "the type slot owns the keys"
    );
    assert!(
        crate::app::bindings::hint(crate::app::bindings::FORM_TYPE_TAIL).contains("[Alt-s] save"),
        "the type slot's inventory must advertise Alt-s save"
    );
    Ok(())
}
