type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::bindings;
use crate::app::Tui;
use crate::context::TuiContext;
use crate::views::entry_form::SlotKind;
use bogita_core::ports::{EncryptedIdentityFile, SqliteStorageFactory};
use bogita_core::test_support::MockKeychain;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::{make_app, make_app_with_entry};

/// Press one unmodified key.
fn key(tui: &mut Tui<MockKeychain, SqliteStorageFactory, EncryptedIdentityFile>, k: KeyCode) {
    tui.handle_key_with_modifiers(k, KeyModifiers::NONE);
}

#[tokio::test]
async fn main_entry_hints_are_generated() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    assert_eq!(tui.status_hint(), bindings::hint(bindings::MAIN_ENTRIES));

    key(&mut tui, KeyCode::Tab); // → detail column
    assert_eq!(tui.status_hint(), bindings::hint(bindings::MAIN_DETAIL));

    key(&mut tui, KeyCode::BackTab); // → entries column
    key(&mut tui, KeyCode::Char(' ')); // leader mode
    assert_eq!(
        tui.status_hint(),
        bindings::hint(bindings::MAIN_LEADER_ENTRIES)
    );
    key(&mut tui, KeyCode::Esc);

    key(&mut tui, KeyCode::Char('v')); // vault picker
    assert_eq!(
        tui.status_hint(),
        bindings::hint(bindings::MAIN_VAULT_PICKER)
    );
    key(&mut tui, KeyCode::Esc);

    key(&mut tui, KeyCode::Char('/')); // search box
    assert_eq!(tui.status_hint(), bindings::hint(bindings::SEARCH));
    Ok(())
}

#[tokio::test]
async fn form_slot_hints_are_generated() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    key(&mut tui, KeyCode::Char(' '));
    key(&mut tui, KeyCode::Char('a')); // add form, name slot

    assert_eq!(
        tui.status_hint(),
        bindings::form_hint(&SlotKind::Name, 0, 0, "", false)
    );

    tui.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT); // → key slot
    assert!(tui
        .status_hint()
        .contains(&bindings::hint(bindings::FORM_KEY)));

    key(&mut tui, KeyCode::Tab); // → type slot
    assert!(tui
        .status_hint()
        .contains(&bindings::hint(bindings::FORM_TYPE_HEAD)));

    key(&mut tui, KeyCode::Tab); // → value slot (Text)
    assert!(tui
        .status_hint()
        .contains(&bindings::hint(bindings::FORM_VALUE)));
    Ok(())
}

#[tokio::test]
async fn modal_hints_are_generated() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    key(&mut tui, KeyCode::Char(' '));
    key(&mut tui, KeyCode::Char('e')); // edit form
    key(&mut tui, KeyCode::Char('x')); // dirty
    key(&mut tui, KeyCode::Esc); // save/discard modal
    assert_eq!(tui.status_hint(), bindings::hint(bindings::CONFIRM_SAVE));
    key(&mut tui, KeyCode::Esc); // back to the form

    key(&mut tui, KeyCode::Esc); // dirty Esc again
    assert_eq!(tui.status_hint(), bindings::hint(bindings::CONFIRM_SAVE));
    Ok(())
}

#[tokio::test]
async fn add_form_discard_modal_hint_is_generated() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    key(&mut tui, KeyCode::Char(' '));
    key(&mut tui, KeyCode::Char('a'));
    key(&mut tui, KeyCode::Char('D'));

    key(&mut tui, KeyCode::Esc); // dirty add form → discard modal

    assert_eq!(tui.status_hint(), bindings::hint(bindings::CONFIRM_DISCARD));
    Ok(())
}

#[tokio::test]
async fn delete_modal_hint_is_generated() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    key(&mut tui, KeyCode::Char(' '));
    key(&mut tui, KeyCode::Char('d')); // delete confirm

    assert_eq!(tui.status_hint(), bindings::hint(bindings::CONFIRM_DELETE));
    Ok(())
}

#[tokio::test]
async fn overlay_hints_are_generated() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Password generator, reached from a token value slot.
    key(&mut tui, KeyCode::Char(' '));
    key(&mut tui, KeyCode::Char('a'));
    tui.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT); // → key slot
    key(&mut tui, KeyCode::Tab); // → type slot
    key(&mut tui, KeyCode::Enter); // open dropdown
    key(&mut tui, KeyCode::Char('j'));
    key(&mut tui, KeyCode::Char('j')); // Token
    key(&mut tui, KeyCode::Enter); // apply
    key(&mut tui, KeyCode::Tab); // → token slot
    tui.handle_key_with_modifiers(KeyCode::Char('g'), KeyModifiers::ALT);
    assert_eq!(tui.status_hint(), bindings::hint(bindings::PASSWORD_GEN));
    key(&mut tui, KeyCode::Esc); // back to the form

    // Vault picker inside the form.
    tui.handle_key_with_modifiers(KeyCode::Char('v'), KeyModifiers::ALT);
    assert_eq!(
        tui.status_hint(),
        bindings::hint(bindings::FORM_VAULT_PICKER)
    );

    Ok(())
}

#[tokio::test]
async fn vault_and_lock_hints_are_generated() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    key(&mut tui, KeyCode::Char('v')); // vault picker
    key(&mut tui, KeyCode::Char('a')); // add vault → vault form
    assert_eq!(
        tui.status_hint(),
        bindings::titled("Editing vault name", bindings::VAULT_FORM_NAME)
    );
    key(&mut tui, KeyCode::Tab); // checkbox cell
    assert_eq!(
        tui.status_hint(),
        bindings::hint(bindings::VAULT_FORM_CHECKBOX)
    );
    key(&mut tui, KeyCode::Esc);

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL); // lock
    assert_eq!(
        tui.status_hint(),
        bindings::titled("Enter passphrase", bindings::LOCKED)
    );
    Ok(())
}

#[tokio::test]
async fn help_overlay_toggles_and_returns_to_the_prior_surface() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    let main_hint = tui.status_hint();

    tui.handle_key_with_modifiers(KeyCode::Char('?'), KeyModifiers::SHIFT);
    assert_eq!(
        tui.status_hint(),
        bindings::titled(bindings::title(bindings::Surface::Help), bindings::HELP)
    );

    key(&mut tui, KeyCode::Esc); // dismiss
    assert_eq!(tui.status_hint(), main_hint);

    tui.handle_key_with_modifiers(KeyCode::Char('?'), KeyModifiers::SHIFT);
    key(&mut tui, KeyCode::Char('q')); // q also dismisses
    assert_eq!(tui.status_hint(), main_hint);
    Ok(())
}

#[test]
fn render_holds_no_hand_written_hints() {
    let render = include_str!("../../src/app/render.rs");
    for literal in [
        "[s/Enter] save",
        "[d] discard",
        "[j/k] select type",
        "Editing token",
        "[/] search",
        "[g] regenerate",
        "Enter passphrase",
        "[Alt-t] toggle default",
    ] {
        assert!(
            !render.contains(literal),
            "render.rs must generate `{literal}` from bindings.rs"
        );
    }
}

#[test]
fn modals_hold_no_hand_written_hints() {
    let modals = include_str!("../../src/app/modals.rs");
    for literal in ["[s/Enter] save", "[d] discard", "back to form", "[b/Esc]"] {
        assert!(
            !modals.contains(literal),
            "modals.rs must generate `{literal}` from bindings.rs"
        );
    }
}
