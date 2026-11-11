use crate::app::{wrap_text, Tui};
use crate::context::TuiContext;
use bogita_core::test_support::test_vault;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;

use super::support::*;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

#[test]
fn wrap_text_keeps_multibyte_characters_intact() -> Result<()> {
    // -- Setup & Fixtures
    let wrapped = wrap_text("é🔒abc", 3);

    // -- Check
    assert_eq!(
        wrapped.concat(),
        "é🔒abc",
        "wrapping must not drop or corrupt multibyte characters"
    );
    assert!(
        wrapped.iter().all(|line| !line.contains('\u{FFFD}')),
        "wrapped lines must not contain replacement characters: {wrapped:?}"
    );
    Ok(())
}

#[test]
fn wrap_text_preserves_long_multibyte_messages() -> Result<()> {
    // -- Setup & Fixtures
    let message = "vault 'café' 🔒 is locked — retry later";
    let wrapped = wrap_text(message, 7);

    // -- Check
    assert_eq!(
        wrapped.concat(),
        message,
        "every character must survive wrapping in order"
    );
    assert!(
        wrapped.iter().all(|line| !line.is_empty()),
        "wrapping must not emit empty lines: {wrapped:?}"
    );
    Ok(())
}

#[tokio::test]
async fn add_entry_context_renders_named_vault() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let vault = test_vault("testvault");
    app.registry.add_vault(&vault).await?;
    let mut tui = Tui::new(
        app,
        TuiContext::AddEntry {
            name: None,
            vault: Some("testvault".to_string()),
        },
    )
    .await?;

    // -- Exec
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| tui.render(frame))?;

    // -- Check
    let rendered: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(
        rendered.contains("Vault: testvault"),
        "add form should show the requested vault, rendered: {rendered}"
    );
    Ok(())
}

#[test]
fn modal_rect_is_capped_and_inside_the_body() -> Result<()> {
    use crate::app::form_modal_rect;
    use ratatui::layout::Rect;

    // -- Exec
    let body = Rect::new(0, 3, 200, 50);
    let rect = form_modal_rect(body, 3);

    // -- Check
    assert!(
        rect.width <= body.width * 6 / 10 + 1,
        "the entry form modal must stay at or under 60% of the body, got: {rect:?}"
    );
    assert!(rect.width >= 40, "the modal must stay readable: {rect:?}");
    assert!(
        rect.x >= body.x && rect.x + rect.width <= body.x + body.width,
        "the modal must stay inside the frame horizontally: {rect:?}"
    );
    assert!(
        rect.y >= body.y && rect.y + rect.height <= body.y + body.height,
        "the modal must stay inside the frame vertically: {rect:?}"
    );
    Ok(())
}

#[test]
fn modal_height_follows_field_count() -> Result<()> {
    use crate::app::form_modal_rect;
    use ratatui::layout::Rect;

    // -- Exec
    let body = Rect::new(0, 0, 160, 60);
    let one = form_modal_rect(body, 1);
    let four = form_modal_rect(body, 4);

    // -- Check
    assert_eq!(one.height, 7 + 3, "one field row hugs its content: {one:?}");
    assert_eq!(
        four.height,
        7 + 3 * 4,
        "four field rows grow the modal: {four:?}"
    );
    assert!(four.height > one.height);
    Ok(())
}

#[tokio::test]
async fn vault_form_checkbox_hint_advertises_alt_t() -> Result<()> {
    // -- Setup & Fixtures
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // -- Exec: vault picker → add vault → Tab to the checkbox cell.
    tui.handle_key_with_modifiers(KeyCode::Char('v'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE);

    // -- Check
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::VaultFormCheckbox,
        "the default checkbox cell owns the keys"
    );
    assert!(
        crate::app::bindings::hint(crate::app::bindings::VAULT_FORM_CHECKBOX)
            .contains("[Alt-t] toggle default"),
        "the checkbox inventory advertises Alt-t"
    );
    Ok(())
}
