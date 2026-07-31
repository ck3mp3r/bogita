//! Tests for the password generator TUI view.

use crate::tui::views::password_gen_view::{PasswordGenAction, PasswordGenView};
use ratatui::crossterm::event::KeyCode;

fn make_view() -> PasswordGenView {
    PasswordGenView::new()
}

// ── construction ──────────────────────────────────────────────────────────────

#[test]
fn new_view_has_default_length_20() {
    assert_eq!(make_view().length(), 20);
}

#[test]
fn new_view_has_default_charset_all_enabled() {
    let v = make_view();
    let opts = v.charset_options();
    assert!(opts.uppercase);
    assert!(opts.lowercase);
    assert!(opts.digits);
    assert!(opts.symbols);
}

#[test]
fn new_view_generates_password_on_creation() {
    let v = make_view();
    assert_eq!(v.current_password().len(), 20);
}

// ── key handling — regenerate ─────────────────────────────────────────────────

#[test]
fn g_key_regenerates_password() {
    let mut v = make_view();
    // Run several times — regenerated password should still be correct length
    for _ in 0..5 {
        let action = v.handle_key(KeyCode::Char('g'));
        assert_eq!(action, PasswordGenAction::None);
        assert_eq!(v.current_password().len(), v.length());
    }
}

// ── key handling — accept ─────────────────────────────────────────────────────

#[test]
fn a_key_returns_accept_with_current_password() {
    let mut v = make_view();
    let action = v.handle_key(KeyCode::Char('a'));
    match action {
        PasswordGenAction::Accept(pw) => {
            assert_eq!(pw.len(), 20);
        }
        other => panic!("expected Accept, got {other:?}"),
    }
}

// ── key handling — cancel ─────────────────────────────────────────────────────

#[test]
fn esc_key_returns_cancel() {
    let mut v = make_view();
    assert_eq!(v.handle_key(KeyCode::Esc), PasswordGenAction::Cancel);
}

// ── key handling — length adjustment ─────────────────────────────────────────

#[test]
fn plus_key_increases_length() {
    let mut v = make_view();
    let before = v.length();
    v.handle_key(KeyCode::Char('+'));
    assert_eq!(v.length(), before + 1);
}

#[test]
fn minus_key_decreases_length() {
    let mut v = make_view();
    v.handle_key(KeyCode::Char('+'));
    v.handle_key(KeyCode::Char('+'));
    let before = v.length();
    v.handle_key(KeyCode::Char('-'));
    assert_eq!(v.length(), before - 1);
}

#[test]
fn length_cannot_go_below_1() {
    let mut v = make_view();
    // Drive length down to 1
    for _ in 0..50 {
        v.handle_key(KeyCode::Char('-'));
    }
    assert_eq!(v.length(), 1);
}

#[test]
fn length_cannot_exceed_128() {
    let mut v = make_view();
    for _ in 0..200 {
        v.handle_key(KeyCode::Char('+'));
    }
    assert_eq!(v.length(), 128);
}

// ── key handling — toggle charset options ────────────────────────────────────

#[test]
fn u_key_toggles_uppercase() {
    let mut v = make_view();
    assert!(v.charset_options().uppercase);
    v.handle_key(KeyCode::Char('u'));
    assert!(!v.charset_options().uppercase);
    v.handle_key(KeyCode::Char('u'));
    assert!(v.charset_options().uppercase);
}

#[test]
fn l_key_toggles_lowercase() {
    let mut v = make_view();
    v.handle_key(KeyCode::Char('l'));
    assert!(!v.charset_options().lowercase);
}

#[test]
fn d_key_toggles_digits() {
    let mut v = make_view();
    v.handle_key(KeyCode::Char('d'));
    assert!(!v.charset_options().digits);
}

#[test]
fn s_key_toggles_symbols() {
    let mut v = make_view();
    v.handle_key(KeyCode::Char('s'));
    assert!(!v.charset_options().symbols);
}

#[test]
fn x_key_toggles_avoid_ambiguous() {
    let mut v = make_view();
    assert!(!v.charset_options().avoid_ambiguous);
    v.handle_key(KeyCode::Char('x'));
    assert!(v.charset_options().avoid_ambiguous);
}

// ── entropy ───────────────────────────────────────────────────────────────────

#[test]
fn entropy_bits_is_exposed() {
    let v = make_view();
    assert!(v.entropy_bits() > 0.0);
}
