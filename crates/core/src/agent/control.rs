//! Vendor-extension control channel of the agent daemon.
//!
//! RFC 9987 EXTENSION(27) carries vendor extensions. The agent answers exactly
//! four names and refuses every other one with EXTENSION_FAILURE(28). Extension
//! names carry the implementation domain, as RFC 4251 § 4.2 requires.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Notify;

// region:    --- Protocol

/// Stop request: answers SUCCESS(6), then the daemon drains and unlinks.
pub const STOP_EXTENSION: &str = "bogita-stop@bogita";

/// Status request: answers EXTENSION_RESPONSE(29) with [`status_payload`].
pub const STATUS_EXTENSION: &str = "bogita-status@bogita";

/// Unlock request: the details carry the passphrase. Answers SUCCESS(6) once the
/// ring serves the vault, or FAILURE(5) on a wrong passphrase. Never echoed.
pub const UNLOCK_EXTENSION: &str = "bogita-unlock@bogita";

/// Lock request: no details. Answers SUCCESS(6) and evicts the ring.
pub const LOCK_EXTENSION: &str = "bogita-lock@bogita";

/// Upper bound for waiting on in-flight sessions after a stop request.
pub const GRACEFUL_DRAIN: Duration = Duration::from_secs(5);

/// Payload of [`STATUS_EXTENSION`]: the tail of the status line.
pub fn status_payload(key_count: usize, locked: bool) -> String {
    let state = if locked { "locked" } else { "unlocked" };
    format!("{key_count} keys, {state}")
}

/// True when a status payload reports an unlocked ring.
pub fn reports_unlocked(payload: &str) -> bool {
    payload.ends_with(", unlocked")
}

// endregion: --- Protocol

// region:    --- Control

/// Stop signal plus the count of in-flight sessions.
///
/// The serve loop watches [`Control::stopped`]; every session reports itself
/// through [`Control::session_started`] and [`Control::session_finished`] so the
/// drain after a stop request knows when the last request has been answered.
#[derive(Debug, Default)]
pub struct Control {
    stopping: AtomicBool,
    active: AtomicUsize,
    notify: Notify,
}

impl Control {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the serve loop to stop accepting connections and to drain.
    pub fn request_stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        self.notify.notify_one();
    }

    /// Resolve once [`Control::request_stop`] has been called.
    pub async fn stopped(&self) {
        while !self.is_stopping() {
            self.notify.notified().await;
        }
    }

    /// True once [`Control::request_stop`] has been called.
    pub fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    /// Count a session as in flight.
    pub fn session_started(&self) {
        self.active.fetch_add(1, Ordering::SeqCst);
    }

    /// Drop a session from the in-flight count.
    pub fn session_finished(&self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
        self.notify.notify_one();
    }

    /// Wait until no session is in flight, at most `limit`.
    pub async fn drain(&self, limit: Duration) {
        let drained = async {
            while self.active.load(Ordering::SeqCst) > 0 {
                self.notify.notified().await;
            }
        };
        let _ = tokio::time::timeout(limit, drained).await;
    }
}

// endregion: --- Control

// region:    --- Tests
#[cfg(test)]
#[path = "../../test/agent/control.rs"]
mod control_test;
// endregion: --- Tests
