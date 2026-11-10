type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::{RunState, Tui};
use crate::context::TuiContext;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::*;

// ── exit path: lock on the way out ───────────────────────────────────────────

#[tokio::test]
async fn exit_hook_deletes_the_staged_identity() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // Stage the identity the way a real unlock does.
    let identity = tui
        .app
        .identity
        .clone()
        .ok_or("test app should carry an identity")?;
    tui.app.session.store_identity(&identity)?;
    assert!(
        tui.app.session.get_identity()?.is_some(),
        "identity should be staged before the exit"
    );

    tui.lock_at_exit();

    assert!(
        tui.app.session.get_identity()?.is_none(),
        "the exit hook must delete the staged identity"
    );
    assert!(tui.app.is_locked, "the app should report locked at exit");
    assert_eq!(
        tui.main_view.visible_entries().len(),
        0,
        "entries should be cleared at exit"
    );
    assert!(tui.detail_entry.is_none(), "detail entry should be cleared");
    Ok(())
}

#[tokio::test]
async fn q_quit_without_lock_timeout_locks() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    assert!(
        tui.app.lock_timeout.is_none(),
        "this test needs the idle timer unset"
    );

    let identity = tui
        .app
        .identity
        .clone()
        .ok_or("test app should carry an identity")?;
    tui.app.session.store_identity(&identity)?;

    tui.handle_key_with_modifiers(KeyCode::Char('q'), KeyModifiers::NONE);
    assert_eq!(tui.state, RunState::Quit, "q should quit");

    tui.lock_at_exit();

    assert!(
        tui.app.session.get_identity()?.is_none(),
        "exit must lock even when no idle timer is set"
    );
    Ok(())
}

#[tokio::test]
async fn ctrl_c_quits_and_locks() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    let identity = tui
        .app
        .identity
        .clone()
        .ok_or("test app should carry an identity")?;
    tui.app.session.store_identity(&identity)?;

    tui.handle_key_with_modifiers(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(tui.state, RunState::Quit, "Ctrl-C should quit");

    tui.lock_at_exit();

    assert!(
        tui.app.session.get_identity()?.is_none(),
        "Ctrl-C exit must delete the staged identity"
    );
    Ok(())
}
