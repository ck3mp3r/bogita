type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::{RunState, Tui};
use crate::context::TuiContext;
use bogita_core::agent::control::LOCK_EXTENSION;
use bogita_core::agent::VaultService;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use std::io::Read;
use std::os::unix::net::UnixListener;

use super::support::*;

// ── exit path: lock on the way out ───────────────────────────────────────────

#[tokio::test]
async fn exit_hook_locks_the_service() -> Result<()> {
    let app = make_app_with_entry().await?;
    let dir = tempfile::tempdir()?;
    let mut tui = Tui::new(app, TuiContext::Default)
        .await?
        .with_daemon_socket(dir.path().join("agent.sock"));
    assert!(
        !tui.service.is_locked(),
        "the fixture boots with an identity"
    );

    tui.lock_at_exit();

    assert!(
        tui.service.is_locked(),
        "the exit hook must lock the service"
    );
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
    let dir = tempfile::tempdir()?;
    let mut tui = Tui::new(app, TuiContext::Default)
        .await?
        .with_daemon_socket(dir.path().join("agent.sock"));
    assert!(
        tui.lock_timeout.is_none(),
        "this test needs the idle timer unset"
    );

    tui.handle_key_with_modifiers(KeyCode::Char('q'), KeyModifiers::NONE);
    assert_eq!(tui.state, RunState::Quit, "q should quit");

    tui.lock_at_exit();

    assert!(
        tui.service.is_locked(),
        "exit must lock even when no idle timer is set"
    );
    Ok(())
}

#[tokio::test]
async fn ctrl_c_quits_and_locks() -> Result<()> {
    let app = make_app_with_entry().await?;
    let dir = tempfile::tempdir()?;
    let mut tui = Tui::new(app, TuiContext::Default)
        .await?
        .with_daemon_socket(dir.path().join("agent.sock"));

    tui.handle_key_with_modifiers(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(tui.state, RunState::Quit, "Ctrl-C should quit");

    tui.lock_at_exit();

    assert!(tui.service.is_locked(), "Ctrl-C exit must lock the service");
    Ok(())
}

#[tokio::test]
async fn exit_hook_sends_the_daemon_lock_request() -> Result<()> {
    // -- Setup & Fixtures: a listener standing in for the daemon socket
    let dir = tempfile::tempdir()?;
    let socket = dir.path().join("agent.sock");
    let listener = UnixListener::bind(&socket)?;
    let daemon = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let (mut stream, _) = listener.accept()?;
        let mut buffer = [0u8; 512];
        let read = stream.read(&mut buffer)?;
        Ok(buffer[..read].to_vec())
    });

    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default)
        .await?
        .with_daemon_socket(&socket);

    // -- Exec
    tui.lock_at_exit();

    // -- Check: the request on the wire names the daemon's lock extension
    let request = daemon.join().map_err(|_| "the daemon thread panicked")??;
    let request = String::from_utf8_lossy(&request);
    assert!(
        request.contains(LOCK_EXTENSION),
        "a TUI exit must send {LOCK_EXTENSION}, got {request:?}"
    );
    assert!(
        tui.service.is_locked(),
        "the exit hook still locks the local service"
    );
    Ok(())
}
