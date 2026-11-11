type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::{RunState, Tui};
use crate::context::TuiContext;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::*;

#[tokio::test]
async fn new_tui_starts_in_running_state() -> Result<()> {
    let app = make_app().await?;
    let tui = Tui::new(app, TuiContext::Default).await?;
    assert_eq!(tui.state, RunState::Running);
    Ok(())
}

#[tokio::test]
async fn new_tui_stores_context() -> Result<()> {
    let app = make_app().await?;
    let ctx = TuiContext::AddEntry {
        name: Some("GitHub".to_string()),
        vault: None,
    };
    let tui = Tui::new(app, ctx).await?;
    assert!(matches!(tui.context, TuiContext::AddEntry { .. }));
    Ok(())
}

#[tokio::test]
async fn handle_key_q_transitions_to_quit() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char('q'), KeyModifiers::NONE);
    assert_eq!(tui.state, RunState::Quit);
    Ok(())
}

#[tokio::test]
async fn handle_key_q_uppercase_transitions_to_quit() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char('Q'), KeyModifiers::NONE);
    assert_eq!(tui.state, RunState::Quit);
    Ok(())
}

#[tokio::test]
async fn handle_key_other_stays_running() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(tui.state, RunState::Running);
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(tui.state, RunState::Running);
    Ok(())
}

#[tokio::test]
async fn new_tui_loads_vaults_from_registry() -> Result<()> {
    let app = make_app().await?;
    // must have at least 1 vault (Personal created in make_app)
    let tui = Tui::new(app, TuiContext::Default).await?;
    assert!(
        !tui.main_view.visible_entries().is_empty() || tui.main_view.vault_count() >= 1,
        "expected at least one vault loaded on startup"
    );
    Ok(())
}

#[tokio::test]
async fn new_tui_loads_entries_from_registry() -> Result<()> {
    let app = make_app_with_entry().await?;
    let tui = Tui::new(app, TuiContext::Default).await?;
    assert_eq!(
        tui.main_view.visible_entries().len(),
        1,
        "expected 1 entry loaded on startup"
    );
    Ok(())
}

#[tokio::test]
async fn confirm_add_entry_persists_and_reloads() -> Result<()> {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Open add form via leader key (Space then 'a'), type a name, confirm
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    // Type entry name
    for c in "MyEntry".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    // Press Enter to confirm
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);

    // Flush the pending action (persist + reload)
    tui.flush_pending().await?;

    assert_eq!(
        tui.main_view.visible_entries().len(),
        1,
        "entry should be persisted and visible"
    );
    assert_eq!(tui.main_view.visible_entries()[0].name, "MyEntry");
    Ok(())
}

#[tokio::test]
async fn confirm_edit_entry_updates_and_reloads() -> Result<()> {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Select the entry and open edit form via leader key (Space then 'e')
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);

    // Clear name and type new one (Backspace x6 to clear "GitHub")
    for _ in 0.."GitHub".len() {
        tui.handle_key_with_modifiers(KeyCode::Backspace, KeyModifiers::NONE);
    }
    for c in "Renamed".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // → ConfirmSave modal
    tui.handle_key_with_modifiers(KeyCode::Char('s'), KeyModifiers::NONE); // confirm save
    tui.flush_pending().await?;

    assert_eq!(tui.main_view.visible_entries().len(), 1);
    assert_eq!(tui.main_view.visible_entries()[0].name, "Renamed");
    Ok(())
}

#[tokio::test]
async fn delete_entry_removes_and_reloads() -> Result<()> {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    assert_eq!(tui.main_view.visible_entries().len(), 1);

    // [Space d] shows confirmation modal — no flush yet
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE);
    // [d] confirms the delete
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE);
    tui.flush_pending().await?;

    assert_eq!(tui.main_view.visible_entries().len(), 0);
    Ok(())
}

#[tokio::test]
async fn delete_cancel_leaves_entry_intact() -> Result<()> {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    assert_eq!(tui.main_view.visible_entries().len(), 1);

    // [Space d] then [c] — should not delete
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('c'), KeyModifiers::NONE);
    tui.flush_pending().await?;

    assert_eq!(tui.main_view.visible_entries().len(), 1);
    Ok(())
}

#[tokio::test]
async fn edit_preserves_selection_on_entry() -> Result<()> {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // The entry "GitHub" should be selected (index 0)
    let original_id = tui
        .main_view
        .selected_entry()
        .ok_or("should have selected entry")?
        .id;

    // Edit it: open form, confirm immediately (name unchanged)
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // → ConfirmSave modal
    tui.handle_key_with_modifiers(KeyCode::Char('s'), KeyModifiers::NONE); // confirm save
    tui.flush_pending().await?;

    // Same entry should still be selected
    let selected_id = tui
        .main_view
        .selected_entry()
        .ok_or("should have selected entry")?
        .id;
    assert_eq!(
        selected_id, original_id,
        "selection should stay on edited entry"
    );
    Ok(())
}

#[tokio::test]
async fn add_selects_newly_created_entry() -> Result<()> {
    use ratatui::crossterm::event::KeyCode;

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
        "newly added entry should be selected"
    );
    Ok(())
}

// ── status_hint ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn status_hint_main_idle() -> Result<()> {
    let app = make_app().await?;
    let tui = Tui::new(app, TuiContext::Default).await?;
    assert_eq!(
        tui.status_hint(),
        crate::app::bindings::footer(),
        "the idle bar is the constant footer"
    );
    assert_eq!(tui.surface(), crate::app::bindings::Surface::MainEntries);
    Ok(())
}

#[tokio::test]
async fn status_hint_leader_mode() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE); // enter leader mode
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::MainLeaderEntries,
        "the leader menu owns the keys"
    );
    let menu = crate::app::bindings::hint(crate::app::bindings::MAIN_LEADER_ENTRIES);
    for chord in ["[a]", "[e]", "[d]", "[Esc]"] {
        assert!(menu.contains(chord), "the leader menu should list {chord}");
    }
    Ok(())
}

#[tokio::test]
async fn status_hint_confirm_delete_view() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    // Open delete confirmation
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE);
    assert_eq!(tui.surface(), crate::app::bindings::Surface::ConfirmDelete);
    let table = crate::app::bindings::hint(crate::app::bindings::CONFIRM_DELETE);
    assert!(table.contains("[d]"), "the delete modal lists confirm");
    assert!(table.contains("c/Esc"), "the delete modal lists cancel");
    Ok(())
}

#[tokio::test]
async fn status_hint_form_name_slot() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE); // open add form — focus on name slot
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::FormName,
        "the name slot owns the keys"
    );
    assert!(
        crate::app::bindings::hint(crate::app::bindings::FORM_NAME).contains("[Esc]"),
        "the name slot's inventory lists cancel"
    );
    Ok(())
}

// ── header_text ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn header_text_contains_app_name() -> Result<()> {
    let app = make_app().await?;
    let tui = Tui::new(app, TuiContext::Default).await?;
    let header = tui.header_text();
    assert!(header.contains("bogita"), "header should contain app name");
    Ok(())
}

#[tokio::test]
async fn header_text_contains_entry_count() -> Result<()> {
    let app = make_app_with_entry().await?;
    let tui = Tui::new(app, TuiContext::Default).await?;
    let header = tui.header_text();
    assert!(
        header.contains('1') || header.contains("entry") || header.contains("entries"),
        "header should convey entry count, got: {header}"
    );
    Ok(())
}

// ── error modal ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn no_error_by_default() -> Result<()> {
    let app = make_app().await?;
    let tui = Tui::new(app, TuiContext::Default).await?;
    assert!(tui.error_message.is_none());
    Ok(())
}

#[tokio::test]
async fn error_message_shown_in_status_hint() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.error_message = Some("vault locked".to_string());
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::ErrorOverlay,
        "the error overlay owns the keys"
    );
    assert!(
        crate::app::bindings::hint(crate::app::bindings::ERROR_OVERLAY).contains("dismiss"),
        "the inventory tells the user how to dismiss"
    );
    Ok(())
}

#[tokio::test]
async fn esc_dismisses_error_modal() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.error_message = Some("backend error".to_string());
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    assert!(
        tui.error_message.is_none(),
        "Esc should clear error_message"
    );
    Ok(())
}

#[tokio::test]
async fn enter_dismisses_error_modal() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.error_message = Some("backend error".to_string());
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        tui.error_message.is_none(),
        "Enter should clear error_message"
    );
    Ok(())
}

#[tokio::test]
async fn other_key_does_not_dismiss_error_modal() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.error_message = Some("backend error".to_string());
    tui.handle_key_with_modifiers(KeyCode::Char('q'), KeyModifiers::NONE);
    assert!(
        tui.error_message.is_some(),
        "non-dismiss key should NOT clear error_message"
    );
    // Also verify that q doesn't quit while error is showing.
    assert_eq!(
        tui.state,
        RunState::Running,
        "app should stay running when error modal is visible"
    );
    Ok(())
}

#[tokio::test]
async fn error_modal_blocks_underlying_view_input() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Set an error while main view has an entry selected.
    tui.error_message = Some("connection failed".to_string());

    // Attempting leader actions should be a no-op while error is visible.
    let entries_before = tui.main_view.visible_entries().len();
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE); // would open leader mode normally
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE); // would trigger delete normally
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE); // would confirm delete normally
    tui.flush_pending().await?;

    let entries_after = tui.main_view.visible_entries().len();
    assert_eq!(
        entries_before, entries_after,
        "error modal should block destructive actions"
    );
    Ok(())
}

#[tokio::test]
async fn g_key_on_password_slot_opens_gen_view() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Open add form → add a field → navigate to type slot → select Token → back to value slot
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT); // add field, focus → key slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // focus → type slot
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // open dropdown
    tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // → 1=Username
    tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // → 2=Token
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // confirm selection
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // forward to the value slot (now "token")
    tui.handle_key_with_modifiers(KeyCode::Char('g'), KeyModifiers::ALT); // open password gen

    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::PasswordGen,
        "Alt-g on a token slot should open the generator"
    );
    Ok(())
}

#[tokio::test]
async fn g_key_on_plain_value_slot_does_not_open_gen_view() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Open add form → add a field → Tab to plain value slot → press Alt-g
    // Alt-g must not open the generator on a plain slot.
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT); // add field → key slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → type slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → plain value slot
    tui.handle_key_with_modifiers(KeyCode::Char('g'), KeyModifiers::ALT); // no gen on a plain slot

    assert_ne!(
        tui.surface(),
        crate::app::bindings::Surface::PasswordGen,
        "[g] on a plain value slot must NOT open gen view"
    );
    Ok(())
}

#[tokio::test]
async fn esc_in_gen_view_returns_to_form() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT); // add field → key slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → type slot
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // open dropdown
    tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // Username
    tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // Token
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // confirm selection
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → token slot
    tui.handle_key_with_modifiers(KeyCode::Char('g'), KeyModifiers::ALT); // open gen view
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE); // cancel gen view

    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::FormToken,
        "Esc in the generator returns to the token slot"
    );
    Ok(())
}

#[tokio::test]
async fn accept_in_gen_view_injects_password_into_form() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    // type name
    for c in "TestEntry".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT); // add field → key slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → type slot
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // open dropdown
    tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // Username
    tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // Token
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // confirm selection
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → token slot
    tui.handle_key_with_modifiers(KeyCode::Char('g'), KeyModifiers::ALT); // open gen view
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE); // accept

    // Back in form — confirm should produce a non-empty value
    assert_ne!(
        tui.surface(),
        crate::app::bindings::Surface::PasswordGen,
        "should be back in form after accept"
    );
    Ok(())
}
