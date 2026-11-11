type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::Tui;
use crate::context::TuiContext;
use bogita_core::domain::{EntryType, FieldType, FieldValue};
use bogita_core::test_support::{test_entry, test_field};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::{make_app, make_app_with_entry};

#[tokio::test]
async fn g_key_on_ssh_key_entry_does_not_open_gen_view() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let vaults = app.registry.list_vaults().await?;
    let vault = vaults.first().ok_or("should have a vault")?;
    let identity = app
        .identity
        .as_ref()
        .ok_or("should have an identity")?
        .clone();
    let svc = app.registry.vault_service_for(vault, identity);
    let mut entry = test_entry(
        vault.id,
        "Deploy key",
        vec![test_field(
            "private_key",
            FieldValue::Hidden("not-a-real-key".to_string()),
            FieldType::SshPrivateKey,
            true,
            0,
        )],
    );
    entry.entry_type = EntryType::SshKey;
    svc.save_entry(&entry).await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // -- Exec: open edit form, Tab three times to the private_key value slot, then [g]
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('g'), KeyModifiers::ALT);

    // -- Check
    assert_ne!(
        tui.surface(),
        crate::app::bindings::Surface::PasswordGen,
        "[g] on an SshKey entry must NOT open the generator"
    );
    Ok(())
}

#[tokio::test]
async fn g_key_on_token_entry_edit_opens_gen_view() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // -- Exec: open edit form, Tab three times to the Token value slot, then [g]
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('g'), KeyModifiers::ALT);

    // -- Check
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::PasswordGen,
        "[g] on a Token entry must open the generator"
    );
    Ok(())
}
