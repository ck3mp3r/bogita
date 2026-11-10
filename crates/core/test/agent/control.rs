//! Tests for the agent control channel payloads (agent/control.rs)
//!
//! Pure: the status payload is a string, nothing is bound or connected.

use crate::agent::control::status_payload;

#[test]
fn status_payload_reports_served_keys_and_unlocked_state() {
    assert_eq!(status_payload(3, false), "3 keys, unlocked");
}

#[test]
fn status_payload_reports_locked_state_without_keys() {
    assert_eq!(status_payload(0, true), "0 keys, locked");
}
