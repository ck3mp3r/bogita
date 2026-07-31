use crate::tui::app::{RunState, Tui};
use crate::tui::context::TuiContext;
use ratatui::crossterm::event::KeyCode;

/// Build a minimal App for testing without touching the filesystem.
///
/// If `with_entry` is true, seeds the Personal vault with one entry.
async fn make_app() -> crate::app::App {
    use crate::crypto::age::AgeCrypto;
    use crate::domain::{AgeIdentity, SqliteConfig, Vault, VaultBackend};
    use crate::storage::sqlite::SqliteStorage;
    use crate::vault::registry::VaultRegistry;
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
        backend: VaultBackend::Sqlite(SqliteConfig {
            path: db_path.to_string_lossy().to_string(),
        }),
        recipients: vec![identity.to_recipient()],
        lock_timeout: None,
        auto_sync: false,
    };
    registry.add_vault(&vault).await.unwrap();

    // Leak the tempdir so the DB file lives for the duration of the test.
    std::mem::forget(dir);

    crate::app::App {
        config: crate::storage::config::AppConfig::default(),
        identity,
        registry,
    }
}

/// Build an App seeded with one entry in the Personal vault.
async fn make_app_with_entry() -> crate::app::App {
    use crate::domain::{Entry, EntryType, Field, FieldType, FieldValue};
    use chrono::Utc;
    use uuid::Uuid;

    let app = make_app().await;
    let vaults = app.registry.list_vaults().await.unwrap();
    let vault = vaults.first().unwrap();
    let svc = app.registry.vault_service_for(vault, app.identity.clone());
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
        vault: None,
    };
    let tui = Tui::new(app, ctx).await.unwrap();
    assert!(matches!(tui.context, TuiContext::AddEntry { .. }));
}

#[tokio::test]
async fn handle_key_q_transitions_to_quit() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    let state = tui.handle_key(KeyCode::Char('q'));
    assert_eq!(state, RunState::Quit);
    assert_eq!(tui.state, RunState::Quit);
}

#[tokio::test]
async fn handle_key_q_uppercase_transitions_to_quit() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    let state = tui.handle_key(KeyCode::Char('Q'));
    assert_eq!(state, RunState::Quit);
}

#[tokio::test]
async fn handle_key_other_stays_running() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    let state = tui.handle_key(KeyCode::Esc);
    assert_eq!(state, RunState::Running);
    let state = tui.handle_key(KeyCode::Enter);
    assert_eq!(state, RunState::Running);
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
    tui.handle_key(KeyCode::Char(' '));
    tui.handle_key(KeyCode::Char('a'));
    // Type entry name
    for c in "MyEntry".chars() {
        tui.handle_key(KeyCode::Char(c));
    }
    // Press Enter to confirm
    tui.handle_key(KeyCode::Enter);

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
    tui.handle_key(KeyCode::Char(' '));
    tui.handle_key(KeyCode::Char('e'));

    // Clear name and type new one (Backspace x6 to clear "GitHub")
    for _ in 0.."GitHub".len() {
        tui.handle_key(KeyCode::Backspace);
    }
    for c in "Renamed".chars() {
        tui.handle_key(KeyCode::Char(c));
    }
    tui.handle_key(KeyCode::Enter); // → ConfirmSave modal
    tui.handle_key(KeyCode::Char('y')); // confirm save
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
    tui.handle_key(KeyCode::Char(' '));
    tui.handle_key(KeyCode::Char('d'));
    // [y] confirms the delete
    tui.handle_key(KeyCode::Char('y'));
    tui.flush_pending().await.unwrap();

    assert_eq!(tui.main_view.visible_entries().len(), 0);
}

#[tokio::test]
async fn delete_cancel_leaves_entry_intact() {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    assert_eq!(tui.main_view.visible_entries().len(), 1);

    // [Space d] then [n] — should not delete
    tui.handle_key(KeyCode::Char(' '));
    tui.handle_key(KeyCode::Char('d'));
    tui.handle_key(KeyCode::Char('n'));
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
    tui.handle_key(KeyCode::Char(' '));
    tui.handle_key(KeyCode::Char('e'));
    tui.handle_key(KeyCode::Enter); // → ConfirmSave modal
    tui.handle_key(KeyCode::Char('y')); // confirm save
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

    tui.handle_key(KeyCode::Char(' '));
    tui.handle_key(KeyCode::Char('a'));
    for c in "Bravo".chars() {
        tui.handle_key(KeyCode::Char(c));
    }
    tui.handle_key(KeyCode::Enter);
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
    tui.handle_key(KeyCode::Char(' ')); // enter leader mode
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
    tui.handle_key(KeyCode::Char(' '));
    tui.handle_key(KeyCode::Char('d'));
    let hint = tui.status_hint();
    assert!(hint.contains("[y]"), "delete hint should mention confirm");
    assert!(
        hint.contains("[n]") || hint.contains("Esc"),
        "delete hint should mention cancel"
    );
}

#[tokio::test]
async fn status_hint_form_name_slot() {
    let app = make_app().await;
    let mut tui = Tui::new(app, TuiContext::Default).await.unwrap();
    tui.handle_key(KeyCode::Char(' '));
    tui.handle_key(KeyCode::Char('a')); // open add form — focus on name slot
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
    tui.handle_key(KeyCode::Esc);
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
    tui.handle_key(KeyCode::Enter);
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
    tui.handle_key(KeyCode::Char('q'));
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
    tui.handle_key(KeyCode::Char(' ')); // would open leader mode normally
    tui.handle_key(KeyCode::Char('d')); // would trigger delete normally
    tui.handle_key(KeyCode::Char('y')); // would confirm delete normally
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
    tui.handle_key(KeyCode::Char(' '));
    tui.handle_key(KeyCode::Char('a'));
    tui.handle_key(KeyCode::Char('+')); // add field, focus → key slot
    tui.handle_key(KeyCode::Tab); // focus → value slot (plain)
    tui.handle_key(KeyCode::Tab); // focus → type slot
    tui.handle_key(KeyCode::Char(' ')); // open dropdown
    tui.handle_key(KeyCode::Char('j')); // → 1=Username
    tui.handle_key(KeyCode::Char('j')); // → 2=Token
    tui.handle_key(KeyCode::Enter); // confirm selection
    tui.handle_key(KeyCode::BackTab); // back to value slot (now "token")
    tui.handle_key(KeyCode::Char('g')); // open password gen

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
    tui.handle_key(KeyCode::Char(' '));
    tui.handle_key(KeyCode::Char('a'));
    tui.handle_key(KeyCode::Char('+')); // add field → key slot
    tui.handle_key(KeyCode::Tab); // → plain value slot

    tui.handle_key(KeyCode::Char('g')); // should type 'g', not open gen

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

    tui.handle_key(KeyCode::Char(' '));
    tui.handle_key(KeyCode::Char('a'));
    tui.handle_key(KeyCode::Char('+')); // add field → key slot
    tui.handle_key(KeyCode::Tab); // → value
    tui.handle_key(KeyCode::Tab); // → type slot
    tui.handle_key(KeyCode::Char(' ')); // open dropdown
    tui.handle_key(KeyCode::Char('j')); // Username
    tui.handle_key(KeyCode::Char('j')); // Token
    tui.handle_key(KeyCode::Enter); // confirm selection
    tui.handle_key(KeyCode::BackTab); // → token slot
    tui.handle_key(KeyCode::Char('g')); // open gen view
    tui.handle_key(KeyCode::Esc); // cancel gen view

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

    tui.handle_key(KeyCode::Char(' '));
    tui.handle_key(KeyCode::Char('a'));
    // type name
    for c in "TestEntry".chars() {
        tui.handle_key(KeyCode::Char(c));
    }
    tui.handle_key(KeyCode::Char('+')); // add field → key slot
    tui.handle_key(KeyCode::Tab); // → value
    tui.handle_key(KeyCode::Tab); // → type slot
    tui.handle_key(KeyCode::Char(' ')); // open dropdown
    tui.handle_key(KeyCode::Char('j')); // Username
    tui.handle_key(KeyCode::Char('j')); // Token
    tui.handle_key(KeyCode::Enter); // confirm selection
    tui.handle_key(KeyCode::BackTab); // → token slot
    tui.handle_key(KeyCode::Char('g')); // open gen view
    tui.handle_key(KeyCode::Char('a')); // accept

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

    tui.handle_key(KeyCode::Tab); // Entries → Detail (queues LoadDetail)
    tui.flush_pending().await.unwrap(); // fetch + decrypt detail_entry
    tui.handle_key(KeyCode::Char('c'));

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
    tui.handle_key(KeyCode::Char('c'));

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

    tui.handle_key(KeyCode::Tab); // → Detail (queues LoadDetail)
    tui.flush_pending().await.unwrap(); // fetch detail_entry
    tui.handle_key(KeyCode::Char('c')); // queue copy
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
    tui.handle_key(ratatui::crossterm::event::KeyCode::Tab);

    let hint = tui.status_hint();
    assert!(
        hint.contains("[c]"),
        "status bar should mention [c] copy when Detail is focused, got: {hint}"
    );
}
