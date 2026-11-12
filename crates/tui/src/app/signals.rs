//! Exit signals for the TUI.
//!
//! `ratatui::init` puts the terminal in raw mode, so Ctrl-C arrives as a key
//! event, not as SIGINT (`keys.rs` maps it). The signals handled here are the
//! ones the process receives from the outside: SIGINT from `kill -INT`, SIGHUP
//! when the terminal closes, and SIGTERM from `kill`. Each one must reach the
//! exit tail in `tui.rs`, so the vault is locked instead of a bare process death.
//!
//! SIGKILL (`kill -9`) cannot be caught: it leaves the vault unlocked in the
//! daemon. The auto-lock timer and `bogita lock` cover that case.
//!
//! The handler only stores a flag — the one thing that is async-signal-safe
//! here — and the event loop polls it on every tick.

use std::sync::atomic::{AtomicBool, Ordering};

/// Signals that map onto the TUI exit tail.
const TERMINATION_SIGNALS: [libc::c_int; 3] = [libc::SIGINT, libc::SIGHUP, libc::SIGTERM];

static TERMINATION_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Install the SIGINT, SIGHUP and SIGTERM handlers.
pub fn install() -> std::io::Result<()> {
    install_for(&TERMINATION_SIGNALS)
}

/// Install the handler for each signal in `signals`.
pub fn install_for(signals: &[libc::c_int]) -> std::io::Result<()> {
    for signal in signals {
        // SAFETY: `signal` installs a handler for one signal number. The handler
        // only stores into a lock-free atomic, which is async-signal-safe; the
        // previous handler's return value is checked for `SIG_ERR` below.
        let previous = unsafe {
            libc::signal(
                *signal,
                record_termination as *const () as libc::sighandler_t,
            )
        };
        if previous == libc::SIG_ERR {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// True once SIGINT, SIGHUP or SIGTERM arrived.
pub fn termination_requested() -> bool {
    TERMINATION_REQUESTED.load(Ordering::SeqCst)
}

/// Signal handler: record the request and return.
extern "C" fn record_termination(_signal: libc::c_int) {
    TERMINATION_REQUESTED.store(true, Ordering::SeqCst);
}
