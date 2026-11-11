//! Direct-contract tests for the hidden git signing helper.
//! The helper's argv router and the two routing targets.
//!
//! The captured argv lives in `support.rs` with the shared fixtures. The sign and
//! default-key behaviour over a daemon lives in `../ssh_relay.rs` (mock) and
//! `../ssh_sign_relay.rs` (live socket).

use super::ssh_sign_support::*;
use crate::handlers::ssh_sign::{route, HelperAction};
use std::path::PathBuf;

// ── argv router: captured shapes ────────────────────────────────────────────

#[test]
fn captured_sign_argv_with_key_line_routes_to_sign() {
    // -- Exec
    let action = route(&owned(CAPTURED_SIGN_ARGV_KEY_LINE));

    // -- Check
    match action {
        HelperAction::Sign(request) => {
            assert_eq!(request.namespace, "git");
            assert_eq!(
                request.key_file,
                PathBuf::from("/tmp/.git_signing_key_tmpetbaTb")
            );
            assert_eq!(
                request.data_file,
                PathBuf::from("/tmp/.git_signing_buffer_tmp0qGFck")
            );
            assert!(request.use_agent, "the captured key-line shape carries -U");
        }
        HelperAction::Forward(args) => panic!("sign shape forwarded: {args:?}"),
    }
}

#[test]
fn captured_sign_argv_with_key_path_routes_to_sign() {
    // -- Exec
    let action = route(&owned(CAPTURED_SIGN_ARGV_KEY_PATH));

    // -- Check
    match action {
        HelperAction::Sign(request) => {
            assert_eq!(request.namespace, "git");
            assert_eq!(
                request.data_file,
                PathBuf::from("/tmp/.git_signing_buffer_tmpErT5ZI")
            );
            assert!(!request.use_agent, "git omits -U for a key path");
        }
        HelperAction::Forward(args) => panic!("sign shape forwarded: {args:?}"),
    }
}

#[test]
fn captured_find_principals_argv_forwards_verbatim() {
    // -- Exec
    let action = route(&owned(CAPTURED_FIND_PRINCIPALS_ARGV));

    // -- Check
    match action {
        HelperAction::Forward(args) => assert_eq!(args, owned(CAPTURED_FIND_PRINCIPALS_ARGV)),
        HelperAction::Sign(_) => panic!("find-principals treated as a sign invocation"),
    }
}

#[test]
fn captured_check_novalidate_argv_forwards_verbatim() {
    // -- Exec
    let action = route(&owned(CAPTURED_CHECK_NOVALIDATE_ARGV));

    // -- Check
    match action {
        HelperAction::Forward(args) => assert_eq!(args, owned(CAPTURED_CHECK_NOVALIDATE_ARGV)),
        HelperAction::Sign(_) => panic!("check-novalidate treated as a sign invocation"),
    }
}

#[test]
fn unknown_sign_option_forwards_instead_of_misparsing() {
    // -- Setup & Fixtures
    let args = owned(&["-Y", "sign", "-n", "git", "--nope", "-f", "k", "d"]);

    // -- Exec
    let action = route(&args);

    // -- Check
    assert!(matches!(action, HelperAction::Forward(ref forwarded) if forwarded == &args));
}

#[test]
fn source_verify_argv_forwards_verbatim() {
    // -- Exec
    let action = route(&owned(SOURCE_VERIFY_ARGV));

    // -- Check
    match action {
        HelperAction::Forward(args) => assert_eq!(args, owned(SOURCE_VERIFY_ARGV)),
        HelperAction::Sign(_) => panic!("verify treated as a sign invocation"),
    }
}

#[test]
fn verify_with_revocation_forwards_the_single_time_token() {
    // -- Setup & Fixtures
    let args = owned(&[
        "-Y",
        "verify",
        "-n",
        "git",
        "-f",
        "/tmp/allowed_signers",
        "-I",
        "alice@example.com",
        "-s",
        "/tmp/sig",
        "-Overify-time=20261109135104",
        "-r",
        "/tmp/revoked_keys",
    ]);

    // -- Exec
    let action = route(&args);

    // -- Check
    match action {
        HelperAction::Forward(forwarded) => assert_eq!(forwarded, args),
        HelperAction::Sign(_) => panic!("verify treated as a sign invocation"),
    }
}

#[test]
fn sign_option_git_never_sends_forwards_instead_of_being_ignored() {
    // -- Setup & Fixtures
    let args = owned(&[
        "-Y",
        "sign",
        "-n",
        "git",
        "-f",
        "k",
        "-O",
        "hashalg=sha256",
        "d",
    ]);

    // -- Exec
    let action = route(&args);

    // -- Check
    assert!(matches!(action, HelperAction::Forward(ref forwarded) if forwarded == &args));
}

#[test]
fn sign_without_data_file_forwards() {
    // -- Setup & Fixtures
    let args = owned(&["-Y", "sign", "-n", "git", "-f", "k"]);

    // -- Exec
    let action = route(&args);

    // -- Check
    assert!(matches!(action, HelperAction::Forward(_)));
}
