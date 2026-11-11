//! The helper's lock gate: the daemon's status payload decides, and the gate runs
//! before routing, so no file work happens while the vault is locked.

use super::ssh_sign_support::*;
use crate::handlers::ssh_sign::{gate, prepare, HelperAction, HelperError, LOCKED_MESSAGE};
use std::time::{Duration, Instant};

#[test]
fn gate_on_a_locked_daemon_fails_fast_with_the_contract_message() {
    // -- Setup & Fixtures
    let started = Instant::now();

    // -- Exec
    let result = gate(Some("0 keys, locked"));

    // -- Check
    let elapsed = started.elapsed();
    let error = match result {
        Err(error) => error,
        Ok(()) => panic!("expected a locked error"),
    };
    assert!(matches!(error, HelperError::Locked));
    assert_eq!(error.to_string(), LOCKED_MESSAGE);
    assert!(
        elapsed < Duration::from_secs(1),
        "the gate must answer within 1 s, took {elapsed:?}"
    );
}

#[test]
fn gate_without_a_daemon_reports_the_locked_message() {
    // -- Exec
    let result = gate(None);

    // -- Check
    assert!(
        matches!(result, Err(HelperError::Locked)),
        "no daemon holds no key, so the vault is locked"
    );
}

#[test]
fn gate_on_an_unlocked_daemon_passes() {
    // -- Exec
    let result = gate(Some("1 keys, unlocked"));

    // -- Check
    assert!(result.is_ok());
}

#[test]
fn prepare_gates_before_routing_a_sign_invocation() {
    // -- Exec
    let result = prepare(Some("0 keys, locked"), &owned(CAPTURED_SIGN_ARGV_KEY_LINE));

    // -- Check
    assert!(
        matches!(result, Err(HelperError::Locked)),
        "the gate precedes routing, so no file work happens while locked"
    );
}

#[test]
fn prepare_gates_before_forwarding_a_verify_invocation() {
    // -- Exec
    let result = prepare(None, &owned(CAPTURED_CHECK_NOVALIDATE_ARGV));

    // -- Check
    assert!(matches!(result, Err(HelperError::Locked)));
}

#[test]
fn prepare_returns_the_routed_action() {
    // -- Exec
    let action = prepare(
        Some("1 keys, unlocked"),
        &owned(CAPTURED_SIGN_ARGV_KEY_LINE),
    );

    // -- Check
    assert!(matches!(action, Ok(HelperAction::Sign(_))));
}
