type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::{RunState, Tui};
use crate::context::TuiContext;
use bogita_core::agent::VaultService;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::*;

// ── clipboard copy ([y] in Detail column) ────────────────────────────────────

#[tokio::test]
async fn y_key_in_detail_sets_pending_copy() -> Result<()> {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // Entries → Detail (queues LoadDetail)
    tui.flush_pending().await?; // fetch + decrypt detail_entry
    tui.handle_key_with_modifiers(KeyCode::Char('y'), KeyModifiers::NONE);

    assert!(
        tui.pending_copy.is_some(),
        "[y] in Detail column should set a pending copy action"
    );
    Ok(())
}

#[tokio::test]
async fn y_key_outside_detail_does_not_set_pending_copy() -> Result<()> {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Default focus is Entries — [y] should be a no-op there.
    tui.handle_key_with_modifiers(KeyCode::Char('y'), KeyModifiers::NONE);

    assert!(
        tui.pending_copy.is_none(),
        "[y] outside Detail column should not set pending copy"
    );
    Ok(())
}

#[tokio::test]
async fn flush_copy_clears_pending_copy() -> Result<()> {
    use ratatui::crossterm::event::KeyCode;

    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    tui.handle_key_with_modifiers(KeyCode::Tab, KeyModifiers::NONE); // → Detail (queues LoadDetail)
    tui.flush_pending().await?; // fetch detail_entry
    tui.handle_key_with_modifiers(KeyCode::Char('y'), KeyModifiers::NONE); // queue copy
    assert!(tui.pending_copy.is_some());

    tui.flush_pending().await?;

    assert!(
        tui.pending_copy.is_none(),
        "flush_pending should drain pending_copy"
    );
    Ok(())
}

#[tokio::test]
async fn status_hint_detail_focused_mentions_copy() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Tab to Detail column.
    tui.handle_key_with_modifiers(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE);

    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::MainDetail,
        "the detail pane owns the keys"
    );
    assert!(
        crate::app::bindings::hint(crate::app::bindings::MAIN_DETAIL).contains("[y] copy"),
        "the detail inventory lists copy"
    );
    Ok(())
}

// ── centered_rect ────────────────────────────────────────────────────────────

#[test]
fn centered_rect_returns_centered_rect() -> Result<()> {
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
    Ok(())
}

#[test]
fn centered_rect_small_area() -> Result<()> {
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
    Ok(())
}

#[tokio::test]
async fn form_renders_as_centered_overlay() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Open add form
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);

    // Verify we're in Form view
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::FormName,
        "should be in form view"
    );

    // Verify main view still has vaults visible (render_cols_1_2 is still called)
    assert!(
        tui.main_view.vault_count() >= 1,
        "vault list should still be visible behind form"
    );
    Ok(())
}

#[tokio::test]
async fn main_view_visible_behind_form() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Verify entries are loaded
    assert_eq!(tui.main_view.visible_entries().len(), 1);

    // Open edit form
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);

    // Verify we're in form view
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::FormName,
        "should be in form view"
    );

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
    Ok(())
}

// ── dirty state / discard confirmation ────────────────────────────────────────

#[tokio::test]
async fn esc_unchanged_form_cancels_without_prompt() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Open the edit form and press Esc immediately: nothing changed, so nothing
    // is asked and nothing is written.
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::MainEntries,
        "a clean edit form closes without the save modal"
    );
    assert_eq!(
        tui.status_message
            .as_ref()
            .map(|(message, _)| message.as_str())
            .unwrap_or_default(),
        "No changes to save"
    );
    Ok(())
}

#[tokio::test]
async fn esc_dirty_edit_form_shows_save_and_discard_modal() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Open edit form
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    // Change the name to make it dirty
    for c in "x".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    // Esc on a dirty edit form opens the save/discard modal
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    let hint = tui.status_hint();
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::ConfirmSave,
        "dirty edit form Esc should offer save and discard, footer was: {hint}"
    );
    Ok(())
}

#[tokio::test]
async fn confirm_discard_closes_form() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Open edit form
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    // Make it dirty
    for c in "x".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    // Press Esc → save/discard modal
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    // Press d to confirm discard
    tui.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::NONE);
    let hint = tui.status_hint();
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::MainEntries,
        "after confirming discard, should be back to main view, footer was: {hint}"
    );
    Ok(())
}

#[tokio::test]
async fn cancel_discard_returns_to_form() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Open edit form
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    // Make it dirty
    for c in "x".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    // Press Esc → save/discard modal
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    // Press b to cancel discard
    tui.handle_key_with_modifiers(KeyCode::Char('b'), KeyModifiers::NONE);
    let hint = tui.status_hint();
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::FormName,
        "after cancelling discard, should be back in form, footer was: {hint}"
    );
    Ok(())
}

#[tokio::test]
async fn esc_in_discard_modal_returns_to_form() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Open edit form
    tui.handle_key_with_modifiers(KeyCode::Char(' '), KeyModifiers::NONE);
    tui.handle_key_with_modifiers(KeyCode::Char('e'), KeyModifiers::NONE);
    // Make it dirty
    for c in "x".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    // Press Esc → save/discard modal
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    // Press Esc again to go back to the form
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);
    let hint = tui.status_hint();
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::FormName,
        "Esc in discard modal should return to form, footer was: {hint}"
    );
    Ok(())
}

// ── lock screen ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn ctrl_l_locks_the_tui() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    assert!(!tui.service.is_locked());

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    assert!(tui.service.is_locked(), "Ctrl-L should lock the app");
    assert_eq!(
        tui.main_view.visible_entries().len(),
        0,
        "entries should be cleared after lock"
    );
    assert!(
        tui.detail_entry.is_none(),
        "detail entry should be cleared after lock"
    );
    Ok(())
}

#[tokio::test]
async fn ctrl_l_when_locked_is_noop() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Lock once
    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);
    assert!(tui.service.is_locked());

    // Try locking again — should be a no-op
    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);
    assert!(tui.service.is_locked(), "should still be locked");
    Ok(())
}

#[tokio::test]
async fn esc_on_lock_screen_quits() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);
    tui.handle_key_with_modifiers(KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(tui.state, RunState::Quit, "Esc on lock screen should quit");
    Ok(())
}

#[tokio::test]
async fn enter_on_lock_screen_with_wrong_passphrase_shows_error() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    // Type a wrong passphrase
    for c in "wrong".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    // The unlock is deferred to the flush, the way the event loop drains it.
    tui.flush_pending().await?;

    // Should still be locked (no identity file on disk, so unlock will fail)
    assert!(
        tui.service.is_locked(),
        "should remain locked after wrong passphrase"
    );
    Ok(())
}

#[tokio::test]
async fn lock_screen_status_hint_unlock() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    let hint = tui.status_hint();
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::Locked,
        "the lock screen owns the keys, footer was: {hint}"
    );
    assert!(
        crate::app::bindings::hint(crate::app::bindings::LOCKED).contains("[Esc]"),
        "the lock inventory lists Esc"
    );
    Ok(())
}

#[tokio::test]
async fn lock_screen_clears_entries() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    assert_eq!(tui.main_view.visible_entries().len(), 1);

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    assert_eq!(
        tui.main_view.visible_entries().len(),
        0,
        "entries should be cleared after lock"
    );
    Ok(())
}

#[tokio::test]
async fn lock_screen_typing_accumulates_chars() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    for c in "hello".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }

    // Typing must not leave the lock screen.
    assert_eq!(
        tui.surface(),
        crate::app::bindings::Surface::Locked,
        "should still be on lock screen after typing"
    );
    Ok(())
}

#[tokio::test]
async fn lock_screen_backspace_removes_chars() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    // Type some chars then backspace
    for c in "ab".chars() {
        tui.handle_key_with_modifiers(KeyCode::Char(c), KeyModifiers::NONE);
    }
    tui.handle_key_with_modifiers(KeyCode::Backspace, KeyModifiers::NONE);

    // Enter with remaining char should still fail (no identity file)
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    tui.flush_pending().await?;

    assert!(tui.service.is_locked(), "should remain locked");
    Ok(())
}

#[tokio::test]
async fn lock_screen_enter_with_empty_input_shows_error() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    // Enter with empty input
    tui.handle_key_with_modifiers(KeyCode::Enter, KeyModifiers::NONE);
    tui.flush_pending().await?;

    assert!(
        tui.service.is_locked(),
        "should remain locked with empty passphrase"
    );
    Ok(())
}

#[tokio::test]
async fn lock_screen_does_not_respond_to_q() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);

    // 'q' should be typed into the passphrase input, not quit
    tui.handle_key_with_modifiers(KeyCode::Char('q'), KeyModifiers::NONE);

    assert_eq!(
        tui.state,
        RunState::Running,
        "'q' on lock screen should type, not quit"
    );
    Ok(())
}
