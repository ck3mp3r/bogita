type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::Tui;
use crate::context::TuiContext;
use bogita_core::domain::SshKeyMaterial;
use bogita_core::ports::{EncryptedIdentityFile, SqliteStorageFactory};
use bogita_core::test_support::MockKeychain;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;

use super::support::make_app;

/// Draw the whole app and flatten the screen, so a test can assert what the
/// user actually sees (the footer is constant now; feedback lives in the form).
fn rendered(
    tui: &mut Tui<MockKeychain, SqliteStorageFactory, EncryptedIdentityFile>,
) -> Result<String> {
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

/// Open an add form with one SshKey row focused on its value slot.
async fn ssh_value_form(tui: &mut Tui<MockKeychain, SqliteStorageFactory, EncryptedIdentityFile>) {
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

const GARBAGE_PEM: &str =
    "-----BEGIN OPENSSH PRIVATE KEY-----\nnot actually a key\n-----END OPENSSH PRIVATE KEY-----\n";

#[tokio::test]
async fn invalid_pem_block_save_with_reason() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    ssh_value_form(&mut tui).await;
    tui.handle_paste(GARBAGE_PEM);

    // -- Exec: the save chord must not queue a save
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    tui.flush_pending().await?;

    // -- Check
    assert!(
        tui.main_view.visible_entries().is_empty(),
        "an unparsable key must never reach storage"
    );
    let screen = rendered(&mut tui)?;
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::FormToken,
        "the form stays open on a refused save"
    );
    assert!(
        screen.contains("Field 1") && screen.contains("not a valid OpenSSH private key"),
        "the form row must state the reason and name the field"
    );
    Ok(())
}

#[tokio::test]
async fn valid_pem_allows_save() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    let pem = SshKeyMaterial::keygen_ed25519("form-door@test")?
        .private_pem()
        .to_string();
    ssh_value_form(&mut tui).await;
    tui.handle_paste(&pem);

    // -- Exec
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    tui.flush_pending().await?;

    // -- Check
    assert_eq!(
        tui.main_view.visible_entries().len(),
        1,
        "a valid PEM saves through the form door"
    );
    Ok(())
}

#[tokio::test]
async fn alt_s_is_inert_on_an_invalid_pem() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    ssh_value_form(&mut tui).await;
    tui.handle_paste(GARBAGE_PEM);

    // -- Exec
    tui.handle_key_with_modifiers(KeyCode::Char('s'), KeyModifiers::ALT);
    tui.flush_pending().await?;

    // -- Check
    assert!(tui.main_view.visible_entries().is_empty());
    let screen = rendered(&mut tui)?;
    assert!(
        screen.contains("not a valid OpenSSH private key"),
        "Alt-s must leave the reason visible in the form"
    );
    Ok(())
}
