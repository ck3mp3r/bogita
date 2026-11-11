//!
//! The lock is the service's, and the view follows it: an idle eviction or a lock
//! taken through the service puts the TUI back on its lock screen.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::bindings::Surface;
use crate::app::Tui;
use crate::context::TuiContext;
use bogita_core::agent::VaultService;
use std::time::{Duration, Instant};

use super::support::{make_app_with_entry, TestService};

/// A TUI over an unlocked service holding one decrypted entry.
async fn open_tui(lock_timeout: Option<u64>) -> Result<Tui<TestService>> {
    let app = make_app_with_entry().await?;
    let tui = Tui::new(app, TuiContext::Default)
        .await?
        .with_lock_timeout(lock_timeout);
    Ok(tui)
}

#[tokio::test]
async fn lock_state_idle_cap_locks_and_shows_the_lock_screen() -> Result<()> {
    // -- Setup & Fixtures: the cap elapsed with no keypress
    let mut tui = open_tui(Some(60)).await?;
    assert_eq!(tui.surface(), Surface::MainEntries);
    tui.last_keypress = Instant::now() - Duration::from_secs(61);

    // -- Exec: the tick the event loop runs
    let locked = tui.poll_idle_lock();

    // -- Check: idle-evicted means no identity, and the lock screen owns the keys
    assert!(locked, "the idle cap must lock the vault");
    assert!(
        tui.service.is_locked(),
        "idle eviction must drop the identity, not only the ring"
    );
    assert_eq!(tui.surface(), Surface::Locked);
    assert_eq!(
        tui.main_view.visible_entries().len(),
        0,
        "a locked view holds no decrypted entry"
    );
    Ok(())
}

#[tokio::test]
async fn lock_state_inside_the_cap_keeps_the_vault_open() -> Result<()> {
    // -- Setup & Fixtures
    let mut tui = open_tui(Some(600)).await?;

    // -- Exec
    let locked = tui.poll_idle_lock();

    // -- Check
    assert!(!locked, "a fresh keypress keeps the vault open");
    assert!(!tui.service.is_locked());
    assert_eq!(tui.surface(), Surface::MainEntries);
    assert_eq!(tui.main_view.visible_entries().len(), 1);
    Ok(())
}

#[tokio::test]
async fn lock_state_service_lock_shows_the_lock_screen() -> Result<()> {
    // -- Setup & Fixtures: an unlocked TUI on the main view
    let mut tui = open_tui(None).await?;
    assert_eq!(tui.surface(), Surface::MainEntries);

    // -- Exec: the service owns the lock, so it can lock behind the view
    tui.service.lock();
    tui.sync_lock_view();

    // -- Check: the view follows the service's lock state
    assert_eq!(tui.surface(), Surface::Locked);
    assert_eq!(
        tui.main_view.visible_entries().len(),
        0,
        "the lock screen must not keep decrypted entries"
    );
    assert!(tui.detail_entry.is_none());
    Ok(())
}
