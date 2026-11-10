type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::Tui;
use crate::context::TuiContext;
use bogita_core::domain::{FieldValue, SshKeyMaterial};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::make_app;

/// Open an add form with one SshKey row focused on its value slot.
async fn ssh_form_on_value_slot(
    tui: &mut Tui<
        bogita_core::test_support::MockKeychain,
        bogita_core::ports::SqliteStorageFactory,
        bogita_core::ports::EncryptedIdentityFile,
    >,
) {
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    for c in "Deploy key".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT); // → key slot
    for c in "private_key".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → type slot
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // open dropdown
    for _ in 0..4 {
        tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // → ssh-key
    }
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // apply
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → value slot
}

#[tokio::test]
async fn paste_keeps_newlines_in_ssh_value() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    let pem = SshKeyMaterial::keygen_ed25519("test")?
        .private_pem()
        .to_string();
    ssh_form_on_value_slot(&mut tui).await;

    // -- Exec: one atomic paste, then save the form
    tui.handle_paste(&pem);
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    tui.flush_pending().await?;

    // -- Check: the stored value keeps every PEM line and still validates
    let vaults = tui.app.registry.list_vaults().await?;
    let vault = vaults.first().ok_or("should have a vault")?;
    let identity = tui
        .app
        .identity
        .as_ref()
        .ok_or("should have an identity")?
        .clone();
    let svc = tui.app.registry.vault_service_for(vault, identity);
    let stored = svc
        .list_entries(vault.id, None)
        .await?
        .into_iter()
        .next()
        .ok_or("should have saved the pasted key")?;
    let value = match &stored.fields[0].value {
        FieldValue::Hidden(v) => v.clone(),
        other => return Err(format!("expected a hidden value, got: {other:?}").into()),
    };
    assert_eq!(value, pem, "the paste must keep the PEM line structure");
    assert!(value.lines().count() >= 3, "expected 3 PEM lines");
    SshKeyMaterial::validate(value.clone())
        .map_err(|e| format!("the stored PEM should validate: {e}"))?;
    Ok(())
}

#[tokio::test]
async fn paste_strips_newlines_in_name() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);

    // -- Exec
    tui.handle_paste("two\nlines");
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    tui.flush_pending().await?;

    // -- Check
    let name = tui
        .main_view
        .selected_entry()
        .map(|e| e.name.clone())
        .ok_or("should have saved the entry")?;
    assert_eq!(name, "two lines", "a single-line name strips line breaks");
    Ok(())
}

#[tokio::test]
async fn paste_normalizes_crlf_to_lf() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    ssh_form_on_value_slot(&mut tui).await;

    // -- Exec: CRLF payload, as a Windows terminal or an editor may send it
    let payload = SshKeyMaterial::keygen_ed25519("test")?
        .private_pem()
        .replace('\n', "\r\n");
    tui.handle_paste(&payload);
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
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
    let stored = svc
        .list_entries(vault.id, None)
        .await?
        .into_iter()
        .next()
        .ok_or("should have saved the pasted key")?;
    let value = match &stored.fields[0].value {
        FieldValue::Hidden(v) => v.clone(),
        other => return Err(format!("expected a hidden value, got: {other:?}").into()),
    };
    assert!(
        !value.contains('\r'),
        "no CR may survive a paste, got: {value:?}"
    );
    assert!(value.contains('\n'), "the LF line structure must survive");
    Ok(())
}
