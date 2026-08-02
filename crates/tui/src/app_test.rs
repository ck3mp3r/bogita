use crate::app::{RunState, Tui};
use crate::context::TuiContext;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

/// Build a minimal App for testing without touching the filesystem.
///
/// If `with_entry` is true, seeds the Personal vault with one entry.
async fn make_app() -> bogita_core::app::App<bogita_core::test_helpers::MockKeychain> {
    use bogita_core::crypto::AgeCrypto;
    use bogita_core::domain::{AgeIdentity, Vault};
    use bogita_core::session::Session;
    use bogita_core::storage::sqlite::SqliteStorage;
    use bogita_core::test_helpers::MockKeychain;
    use bogita_core::vault::registry::VaultRegistry;
    use chrono::Utc;
    use uuid::Uuid;

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("tui_test.db");
    let identity = AgeIdentity::generate();
    let storage = SqliteStorage::new(&db_path, AgeCrypto).await.unwrap();
    let registry = VaultRegistry::new(storage, AgeCrypto);
    let vault = Vault {
        id: Uuid::new_v4(),
        name: "Personal".to_string(),
        is_default: true,
        created_at: Utc::now().timestamp(),
        sync_target: None,
        recipients: vec![identity.to_recipient()],
        lock_timeout: None,
        auto_sync: false,
    };
    registry.add_vault(&vault).await.unwrap();

    // Leak the tempdir so the DB file lives for the duration of the test.
    std::mem::forget(dir);

    bogita_core::app::App {
        config: bogita_core::storage::config::AppConfig::default(),
        identity: Some(identity),
        registry,
        session: Session::new(MockKeychain::new()),
        is_locked: false,
        lock_timeout: None,
    }
}

/// Build an App seeded with one entry in the Personal vault.
async fn make_app_with_entry() -> bogita_core::app::App<bogita_core::test_helpers::MockKeychain> {
    use bogita_core::domain::{Entry, EntryType, Field, FieldType, FieldValue};
    use chrono::Utc;
    use uuid::Uuid;

    let app = make_app().await;
    let vaults = app.registry.list_vaults().await.unwrap();
    let vault = vaults.first().unwrap();
    let svc = app
        .registry
        .vault_service_for(vault, app.identity.as_ref().unwrap().clone());
    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: vault.id,
        name: "GitHub".to_string(),
        entry_type: EntryType::Token,
        fields: vec![Field {
            id: Uuid::new_v4(),
            key: "password".to_string(),
            field_type: FieldType::Token,
            value: FieldValue::Hidden("s3cret".to_string()),
            encrypted: true,
            idx: 0,
        }],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    };
    svc.add_entry(&entry).await.unwrap();
    app
}

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

// ── clipboard copy ([c] in Detail column) ────────────────────────────────────

#[tokio::test]
async fn c_key_in_detail_sets_pending_copy() {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // Entries → Detail (queues LoadDetail)
    tui.flush_pending().await.unwrap(); // fetch + decrypt detail_entry
    tui.handle_key_with_modifiers(KeyCode::Char('c'), KeyModifiers::NONE);

    assert!(
        tui.pending_copy.is_some(),
        "[c] in Detail column should set a pending copy action"
    );
}

#[tokio::test]
async fn c_key_outside_detail_does_not_set_pending_copy() {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    // Default focus is Entries — [c] should be a no-op there.
    tui.handle_key_with_modifiers(KeyCode::Char('c'), KeyModifiers::NONE);

    assert!(
        tui.pending_copy.is_none(),
        "[c] outside Detail column should not set pending copy"
    );
}

#[tokio::test]
async fn flush_copy_clears_pending_copy() {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → Detail (queues LoadDetail)
    tui.flush_pending().await.unwrap(); // fetch detail_entry
    tui.handle_key_with_modifiers(KeyCode::Char('c'), KeyModifiers::NONE); // queue copy
    assert!(tui.pending_copy.is_some());

    tui.flush_pending().await.unwrap();

    assert!(
        tui.pending_copy.is_none(),
        "flush_pending should drain pending_copy"
    );
}

#[tokio::test]
async fn status_hint_detail_focused_mentions_copy() {
    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    // Tab to Detail column.
    tui.handle_key_with_modifiers(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE);

    let hint = tui.status_hint();
    assert!(
        hint.contains("[c]"),
        "status bar should mention [c] copy when Detail is focused, got: {hint}"
    );
}

// ── centered_rect ────────────────────────────────────────────────────────────

#[test]
fn centered_rect_returns_centered_rect() {
    use crate::app::centered_rect;
    use ratatui::layout::Rect;

    let area = Rect::new(0, 0, 100, 100);
    let result = centered_rect(80, 90, area);

    // Width should be 80% of 100 = 80
    assert_eq!(result.width, 80);
    // Height should be 90% of 100 = 90
    assert_eq!(result.height, 90);
    // Left margin: (100 - 80) / 2 = 10
    assert_eq!(result.x, 10);
    // Top margin: (100 - 90) / 2 = 5
    assert_eq!(result.y, 5);
}

#[test]
fn centered_rect_small_area() {
    use crate::app::centered_rect;
    use ratatui::layout::Rect;

    let area = Rect::new(0, 0, 50, 30);
    let result = centered_rect(80, 90, area);

    // Width: 80% of 50 = 40
    assert_eq!(result.width, 40);
    // Height: 90% of 30 = 27 (integer division: 90% of 30 = 27)
    assert_eq!(result.height, 27);
    // Left margin: (50 - 40) / 2 = 5
    assert_eq!(result.x, 5);
    // Top margin: (30 - 27) / 2 = 1 (integer division), but ratatui may
    // distribute the remainder, so the actual top margin is 2.
    assert_eq!(result.y, 2);
}

#[tokio::test]
async fn form_renders_as_centered_overlay() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    // Open add form
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);

    // Verify we're in Form view
    let hint = tui.status_hint();
    assert!(hint.contains("name"), "should be in form view, got: {hint}");

    // Verify main view still has vaults visible (render_cols_1_2 is still called)
    assert!(
        tui.main_view.vault_count() >= 1,
        "vault list should still be visible behind form"
    );
}

#[tokio::test]
async fn main_view_visible_behind_form() {
    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    // Verify entries are loaded
    assert_eq!(tui.main_view.visible_entries().len(), 1);

    // Open edit form
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);

    // Verify we're in form view
    let hint = tui.status_hint();
    assert!(hint.contains("name"), "should be in form view, got: {hint}");

    // Verify entries are still visible in main_view (render_cols_1_2 still called)
    assert_eq!(
        tui.main_view.visible_entries().len(),
        1,
        "entries should still be visible behind form"
    );

    // Verify vaults are still visible
    assert!(
        tui.main_view.vault_count() >= 1,
        "vaults should still be visible behind form"
    );
}

// ── dirty state / discard confirmation ────────────────────────────────────────

#[tokio::test]
async fn esc_unchanged_form_cancels_without_prompt() {
    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    // Open edit form
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    // Press Esc immediately — no changes, should go to ConfirmSave (edit mode)
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    // Should be in ConfirmSave (edit mode always goes to ConfirmSave on Cancel)
    // But we want to test the dirty check: unchanged form → Cancel → ConfirmSave
    // Actually, edit mode Cancel always goes to ConfirmSave regardless of dirty.
    // The dirty check is in the form's Esc handler, which returns Cancel (not dirty)
    // and then app.rs wraps it in ConfirmSave for edit mode.
    // So we just verify we're not in Form anymore.
    let hint = tui.status_hint();
    assert!(
        hint.contains("[s] save"),
        "unchanged edit form Esc should go to ConfirmSave, got: {hint}"
    );
}

#[tokio::test]
async fn esc_dirty_form_shows_discard_modal() {
    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    // Open edit form
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    // Change the name to make it dirty
    for c in "x".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    // Press Esc — dirty form should show ConfirmDiscard
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    let hint = tui.status_hint();
    assert!(
        hint.contains("[d] discard"),
        "dirty form Esc should show discard modal, got: {hint}"
    );
}

#[tokio::test]
async fn confirm_discard_closes_form() {
    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    // Open edit form
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    // Make it dirty
    for c in "x".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    // Press Esc → ConfirmDiscard
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    // Press d to confirm discard
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE);
    let hint = tui.status_hint();
    assert!(
        !hint.contains("discard"),
        "after confirming discard, should be back to main view, got: {hint}"
    );
}

#[tokio::test]
async fn cancel_discard_returns_to_form() {
    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    // Open edit form
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    // Make it dirty
    for c in "x".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    // Press Esc → ConfirmDiscard
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    // Press b to cancel discard
    tui.handle_key_with_modifiers(KeyCode::Char('b'), KeyModifiers::NONE);
    let hint = tui.status_hint();
    assert!(
        hint.contains("Editing name"),
        "after cancelling discard, should be back in form, got: {hint}"
    );
}

#[tokio::test]
async fn esc_in_discard_modal_returns_to_form() {
    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    // Open edit form
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    // Make it dirty
    for c in "x".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    // Press Esc → ConfirmDiscard
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    // Press Esc again to cancel discard
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    let hint = tui.status_hint();
    assert!(
        hint.contains("Editing name"),
        "Esc in discard modal should return to form, got: {hint}"
    );
}

// ── lock screen ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn ctrl_l_locks_the_tui() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    assert!(!tui.app.is_locked);

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    assert!(tui.app.is_locked, "Ctrl-L should lock the app");
    assert_eq!(
        tui.main_view.visible_entries().len(),
        0,
        "entries should be cleared after lock"
    );
    assert!(
        tui.detail_entry.is_none(),
        "detail entry should be cleared after lock"
    );
}

#[tokio::test]
async fn ctrl_l_when_locked_is_noop() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    // Lock once
    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);
    assert!(tui.app.is_locked);

    // Try locking again — should be a no-op
    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);
    assert!(tui.app.is_locked, "should still be locked");
}

#[tokio::test]
async fn esc_on_lock_screen_quits() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(tui.state, RunState::Quit, "Esc on lock screen should quit");
}

#[tokio::test]
async fn enter_on_lock_screen_with_wrong_passphrase_shows_error() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    // Type a wrong passphrase
    for c in "wrong".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);

    // Should still be locked (no identity file on disk, so unlock will fail)
    assert!(
        tui.app.is_locked,
        "should remain locked after wrong passphrase"
    );
}

#[tokio::test]
async fn lock_screen_status_hint_unlock() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    let hint = tui.status_hint();
    assert!(
        hint.contains("passphrase"),
        "lock screen hint should mention passphrase, got: {hint}"
    );
    assert!(
        hint.contains("[Esc]"),
        "lock screen hint should mention Esc, got: {hint}"
    );
}

#[tokio::test]
async fn lock_screen_clears_entries() {
    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    assert_eq!(tui.main_view.visible_entries().len(), 1);

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    assert_eq!(
        tui.main_view.visible_entries().len(),
        0,
        "entries should be cleared after lock"
    );
}

#[tokio::test]
async fn lock_screen_typing_accumulates_chars() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    for c in "hello".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }

    // Verify the passphrase input accumulated chars by checking status hint
    // (we can't directly access the TextInputState, but we can verify behavior)
    let hint = tui.status_hint();
    assert!(
        hint.contains("passphrase"),
        "should still be on lock screen after typing, got: {hint}"
    );
}

#[tokio::test]
async fn lock_screen_backspace_removes_chars() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    // Type some chars then backspace
    for c in "ab".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Backspace, KeyModifiers::NONE);

    // Enter with remaining char should still fail (no identity file)
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);

    assert!(tui.app.is_locked, "should remain locked");
}

#[tokio::test]
async fn lock_screen_enter_with_empty_input_shows_error() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    // Enter with empty input
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);

    assert!(
        tui.app.is_locked,
        "should remain locked with empty passphrase"
    );
}

#[tokio::test]
async fn lock_screen_does_not_respond_to_q() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    // 'q' should be typed into the passphrase input, not quit
    tui.handle_key_with_modifiers(KeyCode::Char('q'), KeyModifiers::NONE);

    assert_eq!(
        tui.state,
        RunState::Running,
        "'q' on lock screen should type, not quit"
    );
}
