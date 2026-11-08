//! Tests for the clipboard service.
//!
//! These tests verify state/logic without requiring a real display server.
//! The `copy_with_timeout` function is tested via a stub/in-process path
//! using a mock clipboard backend injected through the generic API.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use secrecy::SecretString;

use crate::service::clipboard::{ClipboardBackend, ClipboardService};

// ── fake backend ──────────────────────────────────────────────────────────────

/// A fake clipboard backend that records set/clear calls in memory.
#[derive(Clone, Default)]
struct FakeClipboard {
    contents: Arc<Mutex<Option<String>>>,
}

impl ClipboardBackend for FakeClipboard {
    fn set_text(&mut self, text: &str) -> crate::error::Result<()> {
        *self.contents.lock().unwrap() = Some(text.to_owned());
        Ok(())
    }

    fn clear(&mut self) -> crate::error::Result<()> {
        *self.contents.lock().unwrap() = None;
        Ok(())
    }
}

// ── construction ──────────────────────────────────────────────────────────────

#[test]
fn service_is_constructable_with_backend() {
    let backend = FakeClipboard::default();
    let _svc = ClipboardService::new(backend);
}

// ── copy ──────────────────────────────────────────────────────────────────────

#[test]
fn copy_sets_text_on_backend() {
    let backend = FakeClipboard::default();
    let contents = Arc::clone(&backend.contents);
    let mut svc = ClipboardService::new(backend);

    let secret = SecretString::from("hunter2".to_owned());
    svc.copy(secret).unwrap();

    assert_eq!(
        contents.lock().unwrap().as_deref(),
        Some("hunter2"),
        "backend should have received the secret text"
    );
}

#[test]
fn copy_empty_string_is_accepted() {
    let backend = FakeClipboard::default();
    let contents = Arc::clone(&backend.contents);
    let mut svc = ClipboardService::new(backend);

    svc.copy(SecretString::from(String::new())).unwrap();

    assert_eq!(
        contents.lock().unwrap().as_deref(),
        Some(""),
        "empty secret should still be placed on the clipboard"
    );
}

// ── clear ─────────────────────────────────────────────────────────────────────

#[test]
fn clear_sets_backend_to_none() {
    let backend = FakeClipboard::default();
    let contents = Arc::clone(&backend.contents);
    let mut svc = ClipboardService::new(backend);

    svc.copy(SecretString::from("secret".to_owned())).unwrap();
    svc.clear().unwrap();

    assert!(
        contents.lock().unwrap().is_none(),
        "clear should remove clipboard contents"
    );
}

#[test]
fn clear_on_empty_clipboard_is_idempotent() {
    let backend = FakeClipboard::default();
    let contents = Arc::clone(&backend.contents);
    let mut svc = ClipboardService::new(backend);

    svc.clear().unwrap();

    assert!(
        contents.lock().unwrap().is_none(),
        "clear on empty clipboard should not error"
    );
}

// ── copy_with_timeout ─────────────────────────────────────────────────────────

#[tokio::test]
async fn copy_with_timeout_sets_text_immediately() {
    let backend = FakeClipboard::default();
    let contents = Arc::clone(&backend.contents);
    let svc = ClipboardService::new(backend);

    let secret = SecretString::from("my-password".to_owned());
    svc.copy_with_timeout(secret, 60).await.unwrap();

    assert_eq!(
        contents.lock().unwrap().as_deref(),
        Some("my-password"),
        "text should be on clipboard immediately after copy_with_timeout"
    );
}

#[tokio::test]
async fn copy_with_timeout_clears_after_delay() {
    let backend = FakeClipboard::default();
    let contents = Arc::clone(&backend.contents);
    let svc = ClipboardService::new(backend);

    let secret = SecretString::from("temp-secret".to_owned());
    // Use a very short timeout so the test is fast.
    svc.copy_with_timeout(secret, 0).await.unwrap();

    // Give the spawned task a moment to clear.
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(
        contents.lock().unwrap().is_none(),
        "clipboard should be cleared after timeout expires"
    );
}
