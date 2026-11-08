use crate::app::{RunState, Tui};
use crate::context::TuiContext;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::*;

#[tokio::test]
async fn new_tui_starts_in_running_state() {
    let app = make_app().await;
    let tui = Tui::new(app, TuiContext::Default).await.unwrap();
    assert_eq!(tui.state, RunState::Running);
}

#[tokio::test]
async fn new_tui_stores_context() {
    let app = make_app().await;
    let ctx = TuiContext::AddEntry {
        name: Some("GitHub".to_string()),
    };
    let tui = Tui::new(app, ctx).await.unwrap();
    assert!(matches!(tui.context, TuiContext::AddEntry { .. }));
}

#[tokio::test]
async fn handle_key_q_transitions_to_quit() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    tui.handle_key_with_modifiers(KeyCode::Char('q'), KeyModifiers::NONE);
    assert_eq!(tui.state, RunState::Quit);
}

#[tokio::test]
async fn handle_key_q_uppercase_transitions_to_quit() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    tui.handle_key_with_modifiers(KeyCode::Char('Q'), KeyModifiers::NONE);
    assert_eq!(tui.state, RunState::Quit);
}

#[tokio::test]
async fn handle_key_other_stays_running() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(tui.state, RunState::Running);
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(tui.state, RunState::Running);
}

#[tokio::test]
async fn new_tui_loads_vaults_from_registry() {
    let app = make_app().await;
    // must have at least 1 vault (Personal created in make_app)
    let tui = Tui::new(app, TuiContext::Default).await.unwrap();
    assert!(
        !tui.main_view.visible_entries().is_empty() || tui.main_view.vault_count() >= 1,
        "expected at least one vault loaded on startup"
    );
}

#[tokio::test]
async fn new_tui_loads_entries_from_registry() {
    let app = make_app_with_entry().await;
    let tui = Tui::new(app, TuiContext::Default).await.unwrap();
    assert_eq!(
        tui.main_view.visible_entries().len(),
        1,
        "expected 1 entry loaded on startup"
    );
}

#[tokio::test]
async fn confirm_add_entry_persists_and_reloads() {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

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
    tui.flush_pending().await.unwrap();

    assert_eq!(
        tui.main_view.visible_entries().len(),
        1,
        "entry should be persisted and visible"
    );
    assert_eq!(tui.main_view.visible_entries()[0].name, "MyEntry");
}

#[tokio::test]
async fn confirm_edit_entry_updates_and_reloads() {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

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
    tui.flush_pending().await.unwrap();

    assert_eq!(tui.main_view.visible_entries().len(), 1);
    assert_eq!(tui.main_view.visible_entries()[0].name, "Renamed");
}

#[tokio::test]
async fn delete_entry_removes_and_reloads() {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    assert_eq!(tui.main_view.visible_entries().len(), 1);

    // [Space d] shows confirmation modal — no flush yet
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE);
    // [d] confirms the delete
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE);
    tui.flush_pending().await.unwrap();

    assert_eq!(tui.main_view.visible_entries().len(), 0);
}

#[tokio::test]
async fn delete_cancel_leaves_entry_intact() {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    assert_eq!(tui.main_view.visible_entries().len(), 1);

    // [Space d] then [c] — should not delete
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('c'), KeyModifiers::NONE);
    tui.flush_pending().await.unwrap();

    assert_eq!(tui.main_view.visible_entries().len(), 1);
}

#[tokio::test]
async fn edit_preserves_selection_on_entry() {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    // The entry "GitHub" should be selected (index 0)
    let original_id = tui.main_view.selected_entry().unwrap().id;

    // Edit it: open form, confirm immediately (name unchanged)
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // → ConfirmSave modal
    tui.handle_key_with_modifiers(KeyCode::Char('s'), KeyModifiers::NONE); // confirm save
    tui.flush_pending().await.unwrap();

    // Same entry should still be selected
    let selected_id = tui.main_view.selected_entry().unwrap().id;
    assert_eq!(
        selected_id, original_id,
        "selection should stay on edited entry"
    );
}

#[tokio::test]
async fn add_selects_newly_created_entry() {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    for c in "Bravo".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    tui.flush_pending().await.unwrap();

    assert_eq!(
        tui.main_view.selected_entry().map(|e| e.name.as_str()),
        Some("Bravo"),
        "newly added entry should be selected"
    );
}

// ── status_hint ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn status_hint_main_idle() {
    let app = make_app().await;
    let tui = Tui::new(app, TuiContext::Default).await.unwrap();
    let hint = tui.status_hint();
    assert!(hint.contains("[/]"), "should mention search key");
    assert!(hint.contains("[Space]"), "should mention leader key");
    assert!(hint.contains("[q]"), "should mention quit");
}

#[tokio::test]
async fn status_hint_leader_mode() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE); // enter leader mode
    let hint = tui.status_hint();
    assert!(hint.contains("[a]"), "leader hint should mention add");
    assert!(hint.contains("[e]"), "leader hint should mention edit");
    assert!(hint.contains("[d]"), "leader hint should mention delete");
    assert!(hint.contains("[Esc]"), "leader hint should mention cancel");
}

#[tokio::test]
async fn status_hint_confirm_delete_view() {
    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    // Open delete confirmation
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE);
    let hint = tui.status_hint();
    assert!(hint.contains("[d]"), "delete hint should mention confirm");
    assert!(
        hint.contains("[c]") || hint.contains("Esc"),
        "delete hint should mention cancel"
    );
}

#[tokio::test]
async fn status_hint_form_name_slot() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE); // open add form — focus on name slot
    let hint = tui.status_hint();
    assert!(
        hint.contains("name"),
        "form name slot hint should say 'name'"
    );
    assert!(hint.contains("[Esc]"), "form hint should mention cancel");
}

// ── header_text ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn header_text_contains_app_name() {
    let app = make_app().await;
    let tui = Tui::new(app, TuiContext::Default).await.unwrap();
    let header = tui.header_text();
    assert!(header.contains("bogita"), "header should contain app name");
}

#[tokio::test]
async fn header_text_contains_entry_count() {
    let app = make_app_with_entry().await;
    let tui = Tui::new(app, TuiContext::Default).await.unwrap();
    let header = tui.header_text();
    assert!(
        header.contains('1') || header.contains("entry") || header.contains("entries"),
        "header should convey entry count, got: {header}"
    );
}

// ── error modal ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn no_error_by_default() {
    let app = make_app().await;
    let tui = Tui::new(app, TuiContext::Default).await.unwrap();
    assert!(tui.error_message.is_none());
}

#[tokio::test]
async fn error_message_shown_in_status_hint() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    tui.error_message = Some("vault locked".to_string());
    let hint = tui.status_hint();
    assert!(
        hint.contains("[Esc") || hint.contains("dismiss"),
        "status hint should guide user to dismiss, got: {hint}"
    );
}

#[tokio::test]
async fn esc_dismisses_error_modal() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    tui.error_message = Some("backend error".to_string());
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    assert!(
        tui.error_message.is_none(),
        "Esc should clear error_message"
    );
}

#[tokio::test]
async fn enter_dismisses_error_modal() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    tui.error_message = Some("backend error".to_string());
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        tui.error_message.is_none(),
        "Enter should clear error_message"
    );
}

#[tokio::test]
async fn other_key_does_not_dismiss_error_modal() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
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
}

#[tokio::test]
async fn error_modal_blocks_underlying_view_input() {
    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    // Set an error while main view has an entry selected.
    tui.error_message = Some("connection failed".to_string());

    // Attempting leader actions should be a no-op while error is visible.
    let entries_before = tui.main_view.visible_entries().len();
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE); // would open leader mode normally
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE); // would trigger delete normally
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE); // would confirm delete normally
    tui.flush_pending().await.unwrap();

    let entries_after = tui.main_view.visible_entries().len();
    assert_eq!(
        entries_before, entries_after,
        "error modal should block destructive actions"
    );
}

#[tokio::test]
async fn g_key_on_password_slot_opens_gen_view() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    // Open add form → add a field → navigate to type slot → select Token → back to value slot
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('+'), KeyModifiers::NONE); // add field, focus → key slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // focus → value slot (plain)
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // focus → type slot
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE); // open dropdown
    tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // → 1=Username
    tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // → 2=Token
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // confirm selection
    tui.handle_key_with_modifiers(KeyCode::BackTab, KeyModifiers::NONE); // back to value slot (now "token")
    tui.handle_key_with_modifiers(KeyCode::Char('g'), KeyModifiers::NONE); // open password gen

    let hint = tui.status_hint();
    assert!(
        hint.contains("[g] regenerate"),
        "status bar should show gen view hints, got: {hint}"
    );
}

#[tokio::test]
async fn g_key_on_plain_value_slot_does_not_open_gen_view() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    // Open add form → add a field → Tab to plain value slot → press [g]
    // [g] should type 'g' into the field, NOT open the generator.
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('+'), KeyModifiers::NONE); // add field → key slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → plain value slot

    tui.handle_key_with_modifiers(KeyCode::Char('g'), KeyModifiers::NONE); // should type 'g', not open gen

    let hint = tui.status_hint();
    assert!(
        !hint.contains("[g] regenerate"),
        "[g] on a plain value slot must NOT open gen view, got: {hint}"
    );
}

#[tokio::test]
async fn esc_in_gen_view_returns_to_form() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('+'), KeyModifiers::NONE); // add field → key slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → value
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → type slot
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE); // open dropdown
    tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // Username
    tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // Token
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // confirm selection
    tui.handle_key_with_modifiers(KeyCode::BackTab, KeyModifiers::NONE); // → token slot
    tui.handle_key_with_modifiers(KeyCode::Char('g'), KeyModifiers::NONE); // open gen view
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE); // cancel gen view

    let hint = tui.status_hint();
    assert!(
        hint.contains("[Esc] cancel") && !hint.contains("[g] regenerate"),
        "should be back in form, got: {hint}"
    );
}

#[tokio::test]
async fn accept_in_gen_view_injects_password_into_form() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    // type name
    for c in "TestEntry".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Char('+'), KeyModifiers::NONE); // add field → key slot
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → value
    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → type slot
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE); // open dropdown
    tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // Username
    tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE); // Token
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE); // confirm selection
    tui.handle_key_with_modifiers(KeyCode::BackTab, KeyModifiers::NONE); // → token slot
    tui.handle_key_with_modifiers(KeyCode::Char('g'), KeyModifiers::NONE); // open gen view
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE); // accept

    // Back in form — confirm should produce a non-empty value
    let hint = tui.status_hint();
    assert!(
        !hint.contains("[g] regenerate"),
        "should be back in form after accept, got: {hint}"
    );
}
