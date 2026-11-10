//! Direct-contract tests for the hidden git signing helper.
//!
//! The argv fixtures are the exact invocations git 2.56.0 sent on this machine
//! (captured with `GIT_TRACE=1` on a scratch repository). They are replayed
//! here so a change in the parser cannot silently break the real contract.

use crate::handlers::ssh_sign::{
    default_key_line, gate, prepare, route, sign_data, HelperAction, HelperError, SignRequest,
    LOCKED_MESSAGE, MULTIPLE_SERVABLE_MESSAGE, NO_MATCH_MESSAGE, NO_SERVABLE_MESSAGE,
};
use bogita_core::domain::{agent_servable_field, AgeIdentity, Entry, EntryType, SshKeyMaterial};
use bogita_core::session::Session;
use bogita_core::test_support::{test_entry, MockKeychain};
use ssh_key::{HashAlg, PublicKey, SshSig};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use uuid::Uuid;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

// ── captured argv fixtures ────────────────────────────────────────────────

/// git sign, `user.signingKey` set to a public key line. Git writes the key
/// line to a temporary file and passes `-U` so the signer uses the key
/// identified by its public half.
const CAPTURED_SIGN_ARGV_KEY_LINE: &[&str] = &[
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
const CAPTURED_SIGN_ARGV_KEY_PATH: &[&str] = &[
    "-Y",
    "sign",
    "-n",
    "git",
    "-f",
    "/tmp/argvprobe/id_ed25519.pub",
    "/tmp/.git_signing_buffer_tmpErT5ZI",
];

/// git verify (`git verify-commit`), principal lookup.
const CAPTURED_FIND_PRINCIPALS_ARGV: &[&str] = &[
    "-Y",
    "find-principals",
    "-f",
    "/Users/christian/.ssh/allowed_signers",
    "-s",
    "/tmp/.git_vtag_tmpw8CrOt",
    "-Overify-time=20261109135104",
];

/// git verify (`git log --show-signature`), signature check without trust.
const CAPTURED_CHECK_NOVALIDATE_ARGV: &[&str] = &[
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
const SOURCE_VERIFY_ARGV: &[&str] = &[
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

fn owned(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| arg.to_string()).collect()
}

fn ssh_entry(name: &str, material: &SshKeyMaterial, servable: bool) -> Entry {
    let mut fields = material.to_fields();
    let idx = fields.len() as i32;
    fields.push(agent_servable_field(servable, idx));
    Entry {
        entry_type: EntryType::SshKey,
        ..test_entry(Uuid::nil(), name, fields)
    }
}

fn write(path: &Path, contents: &str) -> Result<()> {
    std::fs::write(path, contents)?;
    Ok(())
}

fn sig_path(data: &Path) -> PathBuf {
    let mut path = data.as_os_str().to_owned();
    path.push(".sig");
    PathBuf::from(path)
}

fn sign_request(dir: &Path, material: &SshKeyMaterial) -> Result<(SignRequest, PathBuf)> {
    let key_file = dir.join("git_signing_key_tmp");
    let data_file = dir.join("git_signing_buffer_tmp");
    write(&key_file, material.public_line())?;
    write(&data_file, "commit payload\n")?;
    let request = SignRequest {
        namespace: "git".to_string(),
        key_file,
        data_file: data_file.clone(),
        use_agent: true,
    };
    Ok((request, data_file))
}

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

// ── sign path ──────────────────────────────────────────────────────────────

#[test]
fn sign_writes_sshsig_next_to_the_data_file() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let entries = vec![ssh_entry("alice@laptop", &material, false)];
    let (request, data_file) = sign_request(dir.path(), &material)?;
    let expected = sig_path(&data_file);
    assert!(!expected.exists());

    // -- Exec
    sign_data(&entries, &request)?;

    // -- Check
    assert!(
        expected.exists(),
        "signature must sit at <data>.sig exactly"
    );
    assert!(!PathBuf::from(format!("{}.sig.sig", data_file.display())).exists());
    let pem = std::fs::read_to_string(&expected)?;
    assert!(pem.starts_with("-----BEGIN SSH SIGNATURE-----"));
    let signature = SshSig::from_pem(pem.as_bytes())?;
    let public = PublicKey::from_openssh(material.public_line())?;
    public.verify("git", b"commit payload\n", &signature)?;
    Ok(())
}

#[test]
fn sign_resolves_the_key_that_matches_the_requested_public_key() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let other = SshKeyMaterial::keygen_ed25519("bob@work")?;
    let wanted = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let entries = vec![
        ssh_entry("bob@work", &other, true),
        ssh_entry("alice@laptop", &wanted, false),
    ];
    let (request, data_file) = sign_request(dir.path(), &wanted)?;

    // -- Exec
    sign_data(&entries, &request)?;

    // -- Check
    let pem = std::fs::read_to_string(sig_path(&data_file))?;
    let signature = SshSig::from_pem(pem.as_bytes())?;
    let public = PublicKey::from_openssh(wanted.public_line())?;
    public.verify("git", b"commit payload\n", &signature)?;
    let other_public = PublicKey::from_openssh(other.public_line())?;
    assert!(
        other_public
            .verify("git", b"commit payload\n", &signature)
            .is_err(),
        "only the requested key may sign"
    );
    Ok(())
}

#[test]
fn sign_uses_the_namespace_git_asks_for() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let entries = vec![ssh_entry("alice@laptop", &material, true)];
    let (request, data_file) = sign_request(dir.path(), &material)?;

    // -- Exec
    sign_data(&entries, &request)?;

    // -- Check
    let pem = std::fs::read_to_string(sig_path(&data_file))?;
    let signature = SshSig::from_pem(pem.as_bytes())?;
    let public = PublicKey::from_openssh(material.public_line())?;
    public.verify("git", b"commit payload\n", &signature)?;
    assert!(
        public
            .verify("file", b"commit payload\n", &signature)
            .is_err(),
        "the signature is bound to the requested namespace"
    );
    Ok(())
}

#[test]
fn sign_resolves_a_key_file_path_without_use_agent() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let entries = vec![ssh_entry("alice@laptop", &material, false)];
    let key_file = dir.path().join("id_ed25519.pub");
    write(&key_file, material.public_line())?;
    let data_file = dir.path().join("git_signing_buffer_tmp");
    write(&data_file, "commit payload\n")?;
    let request = SignRequest {
        namespace: "git".to_string(),
        key_file,
        data_file: data_file.clone(),
        use_agent: false,
    };

    // -- Exec
    sign_data(&entries, &request)?;

    // -- Check
    let pem = std::fs::read_to_string(sig_path(&data_file))?;
    let signature = SshSig::from_pem(pem.as_bytes())?;
    let public = PublicKey::from_openssh(material.public_line())?;
    public.verify("git", b"commit payload\n", &signature)?;
    Ok(())
}

#[test]
fn sign_without_a_matching_key_reports_the_contract_message() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let stored = SshKeyMaterial::keygen_ed25519("bob@work")?;
    let requested = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let entries = vec![ssh_entry("bob@work", &stored, true)];
    let (request, data_file) = sign_request(dir.path(), &requested)?;

    // -- Exec
    let result = sign_data(&entries, &request);

    // -- Check
    let error = match result {
        Err(error) => error,
        Ok(()) => return Err("expected a missing-key error".into()),
    };
    assert!(matches!(error, HelperError::NoMatchingKey));
    assert_eq!(error.to_string(), NO_MATCH_MESSAGE);
    assert!(!sig_path(&data_file).exists(), "no signature on a miss");
    Ok(())
}

#[test]
fn sign_reports_an_unreadable_key_file() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let entries = vec![ssh_entry("alice@laptop", &material, true)];
    let data_file = dir.path().join("buffer");
    write(&data_file, "payload\n")?;
    let request = SignRequest {
        namespace: "git".to_string(),
        key_file: dir.path().join("missing_key_file"),
        data_file,
        use_agent: true,
    };

    // -- Exec
    let result = sign_data(&entries, &request);

    // -- Check
    assert!(result.is_err());
    Ok(())
}

#[test]
fn signature_parses_as_sshsig_armor() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let entries = vec![ssh_entry("alice@laptop", &material, false)];
    let (request, data_file) = sign_request(dir.path(), &material)?;

    // -- Exec
    sign_data(&entries, &request)?;

    // -- Check
    let pem = std::fs::read_to_string(sig_path(&data_file))?;
    assert!(pem.ends_with("-----END SSH SIGNATURE-----\n"));
    let signature = SshSig::from_pem(pem.as_bytes())?;
    assert_eq!(signature.version(), SshSig::VERSION);
    assert_eq!(signature.namespace(), "git");
    assert_eq!(signature.hash_alg(), HashAlg::Sha512);
    Ok(())
}

// ── default-key subcommand ──────────────────────────────────────────────────

#[test]
fn default_key_prints_the_only_servable_key_line() -> Result<()> {
    // -- Setup & Fixtures
    let wanted = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let other = SshKeyMaterial::keygen_ed25519("bob@work")?;
    let entries = vec![
        ssh_entry("alice@laptop", &wanted, true),
        ssh_entry("bob@work", &other, false),
    ];

    // -- Exec
    let line = default_key_line(&entries)?;

    // -- Check
    assert_eq!(line, wanted.public_line());
    Ok(())
}

#[test]
fn default_key_with_no_servable_key_reports_the_contract_message() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let entries = vec![ssh_entry("alice@laptop", &material, false)];

    // -- Exec
    let result = default_key_line(&entries);

    // -- Check
    let error = match result {
        Err(error) => error,
        Ok(line) => return Err(format!("expected an error, got '{line}'").into()),
    };
    assert!(matches!(error, HelperError::NoAgentServableKeys));
    assert_eq!(error.to_string(), NO_SERVABLE_MESSAGE);
    Ok(())
}

#[test]
fn default_key_with_multiple_servable_keys_refuses_to_pick() -> Result<()> {
    // -- Setup & Fixtures
    let first = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let second = SshKeyMaterial::keygen_ed25519("bob@work")?;
    let entries = vec![
        ssh_entry("alice@laptop", &first, true),
        ssh_entry("bob@work", &second, true),
    ];

    // -- Exec
    let result = default_key_line(&entries);

    // -- Check
    let error = match result {
        Err(error) => error,
        Ok(line) => return Err(format!("expected an error, got '{line}'").into()),
    };
    assert!(matches!(error, HelperError::MultipleAgentServableKeys));
    assert_eq!(error.to_string(), MULTIPLE_SERVABLE_MESSAGE);
    Ok(())
}

#[test]
fn default_key_with_no_entries_errors() {
    // -- Exec
    let result = default_key_line(&[]);

    // -- Check
    assert!(matches!(result, Err(HelperError::NoAgentServableKeys)));
}

// ── identity gate ───────────────────────────────────────────────────────────

#[test]
fn gate_on_a_locked_vault_fails_fast_with_the_contract_message() {
    // -- Setup & Fixtures
    let session = Session::new(MockKeychain::new());
    let started = Instant::now();

    // -- Exec
    let result = gate(&session);

    // -- Check
    let elapsed = started.elapsed();
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("expected a locked error"),
    };
    assert!(matches!(error, HelperError::Locked));
    assert_eq!(error.to_string(), LOCKED_MESSAGE);
    assert!(
        elapsed < Duration::from_secs(1),
        "the gate must answer within 1 s, took {elapsed:?}"
    );
}

#[test]
fn gate_on_an_unlocked_vault_returns_the_identity() -> Result<()> {
    // -- Setup & Fixtures
    let identity = AgeIdentity::generate();
    let session = Session::new(MockKeychain::new());
    session.store_identity(&identity)?;

    // -- Exec
    let loaded = gate(&session)?;

    // -- Check
    assert_eq!(loaded.to_recipient(), identity.to_recipient());
    Ok(())
}

#[test]
fn prepare_gates_before_routing_a_sign_invocation() {
    // -- Setup & Fixtures
    let session = Session::new(MockKeychain::new());

    // -- Exec
    let result = prepare(&owned(CAPTURED_SIGN_ARGV_KEY_LINE), &session);

    // -- Check
    assert!(
        matches!(result, Err(HelperError::Locked)),
        "the gate precedes routing, so no file work happens while locked"
    );
}

#[test]
fn prepare_gates_before_forwarding_a_verify_invocation() {
    // -- Setup & Fixtures
    let session = Session::new(MockKeychain::new());

    // -- Exec
    let result = prepare(&owned(CAPTURED_CHECK_NOVALIDATE_ARGV), &session);

    // -- Check
    assert!(matches!(result, Err(HelperError::Locked)));
}

#[test]
fn prepare_returns_the_identity_and_the_routed_action() -> Result<()> {
    // -- Setup & Fixtures
    let identity = AgeIdentity::generate();
    let session = Session::new(MockKeychain::new());
    session.store_identity(&identity)?;

    // -- Exec
    let (_identity, action) = prepare(&owned(CAPTURED_SIGN_ARGV_KEY_LINE), &session)?;

    // -- Check
    assert!(matches!(action, HelperAction::Sign(_)));
    Ok(())
}
