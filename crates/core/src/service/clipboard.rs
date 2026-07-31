//! Clipboard service.
//!
//! Copies a secret to the system clipboard and optionally clears it after a
//! configurable timeout via a background Tokio task.
//!
//! ## Design
//! - `ClipboardBackend` is a small trait so that tests can inject a fake
//!   in-process clipboard without requiring a real display server.
//! - `ClipboardService<B>` holds the backend and exposes [`copy`],
//!   [`clear`], and [`copy_with_timeout`].
//! - `copy_with_timeout` moves a clone of the backend into a `tokio::spawn`
//!   task that sleeps for `timeout_secs` seconds, then calls `clear`.
//!   The `SecretString` is zeroized when it goes out of scope (via `secrecy`).

use secrecy::{ExposeSecret, SecretString};
use std::time::Duration;

use crate::error::Result;

// ── ClipboardBackend ──────────────────────────────────────────────────────────

/// Abstracts over a real or fake clipboard implementation.
///
/// Implement this trait to inject a test double; the real implementation is
/// [`ArboardBackend`].
pub trait ClipboardBackend: Clone + Send + 'static {
    /// Place `text` on the clipboard.
    fn set_text(&mut self, text: &str) -> Result<()>;
    /// Clear the clipboard (set to empty / remove contents).
    fn clear(&mut self) -> Result<()>;
}

// ── ArboardBackend ────────────────────────────────────────────────────────────

/// Real clipboard backend backed by [`arboard::Clipboard`].
///
/// `arboard::Clipboard` is not `Clone`, so we re-create it on each call.
/// This is slightly expensive but clipboard operations are infrequent.
#[derive(Clone, Default)]
pub struct ArboardBackend;

impl ClipboardBackend for ArboardBackend {
    fn set_text(&mut self, text: &str) -> Result<()> {
        let mut cb = arboard::Clipboard::new()
            .map_err(|e| crate::error::Error::Io(std::io::Error::other(e.to_string())))?;
        cb.set_text(text)
            .map_err(|e| crate::error::Error::Io(std::io::Error::other(e.to_string())))
    }

    fn clear(&mut self) -> Result<()> {
        let mut cb = arboard::Clipboard::new()
            .map_err(|e| crate::error::Error::Io(std::io::Error::other(e.to_string())))?;
        cb.clear()
            .map_err(|e| crate::error::Error::Io(std::io::Error::other(e.to_string())))
    }
}

// ── ClipboardService ──────────────────────────────────────────────────────────

/// Clipboard service — wraps a [`ClipboardBackend`] and provides
/// `copy`, `clear`, and `copy_with_timeout`.
pub struct ClipboardService<B: ClipboardBackend> {
    backend: B,
}

impl<B: ClipboardBackend> ClipboardService<B> {
    /// Create a new service with the given backend.
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Copy `text` to the clipboard immediately (no timeout).
    pub fn copy(&mut self, text: SecretString) -> Result<()> {
        self.backend.set_text(text.expose_secret())
    }

    /// Clear the clipboard.
    pub fn clear(&mut self) -> Result<()> {
        self.backend.clear()
    }

    /// Copy `text` to the clipboard, then spawn a background task that clears
    /// it after `timeout_secs` seconds.
    ///
    /// The `SecretString` is dropped (and therefore zeroized) inside the
    /// spawned task once it has been consumed.
    pub async fn copy_with_timeout(mut self, text: SecretString, timeout_secs: u64) -> Result<()> {
        // Copy immediately.
        self.backend.set_text(text.expose_secret())?;

        // Spawn a task to clear after the timeout.
        let mut backend = self.backend.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(timeout_secs)).await;
            let _ = backend.clear();
            // `text` is dropped here — SecretString zeroizes on drop.
            drop(text);
        });

        Ok(())
    }
}

// ── convenience constructor for the real backend ──────────────────────────────

/// Create a `ClipboardService` backed by the real system clipboard.
pub fn system() -> ClipboardService<ArboardBackend> {
    ClipboardService::new(ArboardBackend)
}
