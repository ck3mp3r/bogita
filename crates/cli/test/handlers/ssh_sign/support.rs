//! Fixtures shared by the git-helper tests: the captured git argv.
//!
//! The argv fixtures are the exact invocations git 2.56.0 sent on this machine
//! (captured with `GIT_TRACE=1` on a scratch repository). They are replayed here
//! so a change in the parser cannot silently break the real contract.

// ── captured argv fixtures ────────────────────────────────────────────────

/// git sign, `user.signingKey` set to a public key line. Git writes the key
/// line to a temporary file and passes `-U` so the signer uses the key
/// identified by its public half.
pub(crate) const CAPTURED_SIGN_ARGV_KEY_LINE: &[&str] = &[
    "-Y",
    "sign",
    "-n",
    "git",
    "-f",
    "/tmp/.git_signing_key_tmpetbaTb",
    "-U",
    "/tmp/.git_signing_buffer_tmp0qGFck",
];

/// git sign, `user.signingKey` set to a path to a `.pub` file. No `-U`.
pub(crate) const CAPTURED_SIGN_ARGV_KEY_PATH: &[&str] = &[
    "-Y",
    "sign",
    "-n",
    "git",
    "-f",
    "/tmp/argvprobe/id_ed25519.pub",
    "/tmp/.git_signing_buffer_tmpErT5ZI",
];

/// git verify (`git verify-commit`), principal lookup.
pub(crate) const CAPTURED_FIND_PRINCIPALS_ARGV: &[&str] = &[
    "-Y",
    "find-principals",
    "-f",
    "/Users/christian/.ssh/allowed_signers",
    "-s",
    "/tmp/.git_vtag_tmpw8CrOt",
    "-Overify-time=20261109135104",
];

/// git verify (`git log --show-signature`), signature check without trust.
pub(crate) const CAPTURED_CHECK_NOVALIDATE_ARGV: &[&str] = &[
    "-Y",
    "check-novalidate",
    "-n",
    "git",
    "-s",
    "/tmp/.git_vtag_tmpw8CrOt",
    "-Overify-time=20261109135104",
];

/// git verify, per-principal stage (`git verify-commit` after
/// `find-principals` returned a principal). Shape from `gpg-interface.c:566`.
pub(crate) const SOURCE_VERIFY_ARGV: &[&str] = &[
    "-Y",
    "verify",
    "-n",
    "git",
    "-f",
    "/Users/christian/.ssh/allowed_signers",
    "-I",
    "alice@example.com",
    "-s",
    "/tmp/.git_vtag_tmpw8CrOt",
    "-Overify-time=20261109135104",
];

// ── helpers ────────────────────────────────────────────────────────────────

pub(crate) fn owned(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| arg.to_string()).collect()
}
