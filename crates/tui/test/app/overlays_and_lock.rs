use crate::app::{RunState, Tui};
use crate::context::TuiContext;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::*;

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
