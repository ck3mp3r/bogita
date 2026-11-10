//! Termination signals for the agent daemon.
//!
//! The daemon started by `bogita agent start` stays in the spawning session's
//! process group and inherits no terminal. Closing that terminal sends SIGHUP;
//! Ctrl-C in the session sends SIGINT. Both must reach the same stop path as
//! `bogita-stop@bogita`, so the daemon drains and unlinks instead of dying with
//! a stale socket file and no chance to finish in-flight requests.
//!
//! The handler only stores a flag — the one thing that is async-signal-safe
//! here — and the serve loop polls it.

use crate::agent::error::SshAgentError;
use std::sync::atomic::{AtomicBool, Ordering};

/// Signals that map onto the stop path.
const TERMINATION_SIGNALS: [libc::c_int; 2] = [libc::SIGINT, libc::SIGHUP];

static TERMINATION_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Install the SIGINT and SIGHUP handlers.
pub fn install() -> Result<(), SshAgentError> {
    for signal in TERMINATION_SIGNALS {
        // SAFETY: `signal` installs a handler for one signal number. The handler
        // only stores into a lock-free atomic, which is async-signal-safe; the
        // previous handler's return value is checked for `SIG_ERR` below.
        let previous = unsafe {
            libc::signal(
                signal,
                record_termination as *const () as libc::sighandler_t,
            )
        };
        if previous == libc::SIG_ERR {
            return Err(SshAgentError::Serve(format!(
                "cannot install the handler for signal {signal}: {}",
                std::io::Error::last_os_error()
            )));
        }
    }
    Ok(())
}

/// True once SIGINT or SIGHUP arrived.
pub fn termination_requested() -> bool {
    TERMINATION_REQUESTED.load(Ordering::SeqCst)
}

/// Signal handler: record the request and return.
extern "C" fn record_termination(_signal: libc::c_int) {
    TERMINATION_REQUESTED.store(true, Ordering::SeqCst);
}
