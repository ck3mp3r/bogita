//! Security tests for the agent daemon (agent/socket.rs, agent/server.rs)
//!
//! Gated on the `integration` feature: these tests bind real unix sockets and
//! redirect the process stderr.
//!
//! Covered here:
//! - the socket directory gate fails closed: a pre-existing directory that is not
//!   0700 refuses the start instead of being widened into safety
//! - the client reports a missing or dead socket as "not running" and never hangs
//! - a full list + sign cycle writes no key material, no signature bytes, and no
//!   field values to stderr
//!
//! Out of scope: process exit codes. `main.rs` maps a `SshAgentError::Socket` from
//! `serve` onto `exit_with_error` (stderr + exit 1); this file asserts the error at
//! the `serve` boundary, which is the value that path consumes.
//!
//! Runner note: the capture tests must behave the same under `cargo test` and under
//! a per-test-process runner. The libtest harness arms a thread-local output sink
//! (`std::io::set_output_capture`) on the test thread and on every thread that
//! thread spawns, so `println!`/`eprintln!` output is routed into that sink and
//! never reaches file descriptor 2 — the descriptor `StderrCapture` replaces. A
//! raw `libc::write` on descriptor 2 bypasses the sink and lands in the capture
//! file, so the marker goes through `StderrCapture::write_marker`. A runner that
//! starts one process per test arms no sink, so the same call works there too. For
//! the same reason the daemon writes the warning this file asserts on with
//! `std::io::stderr()` (`KeyRing`'s `skip`) instead of `eprintln!`: print-macro
//! output is what the sink swallows.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::client;
use crate::agent::error::SshAgentError;
use crate::agent::keyring::{KeyRing, SharedKeyRing};
use crate::agent::server::serve;
use crate::domain::ssh::{agent_servable_field, SshKeyMaterial};
use crate::domain::{Entry, FieldType, FieldValue};
use crate::test_support::{test_entry, test_field};
use ssh_agent_lib::agent::Session;
use ssh_agent_lib::client::Client;
use ssh_agent_lib::proto::{PublicCredential, Request, Response, SignRequest};
use ssh_key::PublicKey;
use std::io::Write;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use uuid::Uuid;

const DATA: &[u8] = b"ssh-agent-sign-request-data";
const STARTUP_LIMIT: usize = 200;

/// Value of the plaintext field that must never reach a log line.
const SECRET_FIELD_KEY: &str = "recovery_note";
const SECRET_FIELD_VALUE: &str = "s5-field-value-do-not-log";

/// A line the test itself writes while the capture is active.
const CAPTURE_MARKER: &str = "s5-capture-marker";

// region:    --- Fixtures

fn ssh_entry(name: &str, material: &SshKeyMaterial) -> Entry {
    let mut fields = material.to_fields();
    fields.push(agent_servable_field(true, fields.len() as i32));
    fields.push(test_field(
        SECRET_FIELD_KEY,
        FieldValue::Text(SECRET_FIELD_VALUE.to_string()),
        FieldType::Custom(SECRET_FIELD_KEY.to_string()),
        false,
        fields.len() as i32,
    ));
    test_entry(Uuid::new_v4(), name, fields)
}

fn locked_ring() -> SharedKeyRing {
    Arc::new(Mutex::new(KeyRing::new()))
}

fn ring_with(entries: Vec<Entry>) -> SharedKeyRing {
    let mut ring = KeyRing::new();
    ring.load_entries(entries);
    Arc::new(Mutex::new(ring))
}

fn temp_socket(dir: &Path) -> PathBuf {
    dir.join("data").join("agent.sock")
}

fn mode_of(path: &Path) -> Result<u32> {
    Ok(std::fs::metadata(path)?.permissions().mode() & 0o777)
}

/// Block until the daemon has bound its socket (or the budget runs out).
async fn wait_for_socket(path: &Path) -> Result<()> {
    for _ in 0..STARTUP_LIMIT {
        if path.exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!("agent socket {} never appeared", path.display()).into())
}

struct RunningAgent {
    handle: JoinHandle<std::result::Result<(), SshAgentError>>,
    socket_path: PathBuf,
}

impl RunningAgent {
    async fn start(ring: SharedKeyRing, socket_path: PathBuf) -> Result<Self> {
        let handle = tokio::spawn(serve(ring, socket_path.clone()));
        wait_for_socket(&socket_path).await?;
        Ok(Self {
            handle,
            socket_path,
        })
    }

    async fn client(&self) -> Result<Client<UnixStream>> {
        Ok(Client::new(UnixStream::connect(&self.socket_path).await?))
    }

    async fn stop(self) {
        self.handle.abort();
        let _ = self.handle.await;
    }
}

// endregion: --- Fixtures

// region:    --- Directory gate

#[tokio::test]
async fn test_agent_security_refuses_a_world_writable_socket_dir() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir)?;
    std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o777))?;
    let socket_path = data_dir.join("agent.sock");

    // -- Exec
    let started = tokio::time::timeout(
        Duration::from_secs(5),
        serve(locked_ring(), socket_path.clone()),
    )
    .await;

    // -- Check
    let Ok(served) = started else {
        return Err(
            "a world-writable socket directory must refuse the start, the daemon kept serving"
                .into(),
        );
    };
    let Err(SshAgentError::Socket(message)) = served else {
        return Err(format!(
            "a world-writable socket directory must refuse the start, got {served:?}"
        )
        .into());
    };
    assert!(
        message.contains("777"),
        "the refusal must name the offending mode, got {message}"
    );
    assert!(
        message.contains("700"),
        "the refusal must name the required mode, got {message}"
    );
    assert!(
        !socket_path.exists(),
        "a refused start must not bind the socket"
    );
    assert_eq!(
        mode_of(&data_dir)?,
        0o777,
        "the refusal must leave the offending directory untouched"
    );
    Ok(())
}

#[tokio::test]
async fn test_agent_security_accepts_an_owner_only_socket_dir() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir)?;
    std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700))?;
    let socket_path = data_dir.join("agent.sock");

    // -- Exec
    let agent = RunningAgent::start(locked_ring(), socket_path.clone()).await?;

    // -- Check
    assert_eq!(
        mode_of(&socket_path)?,
        0o600,
        "the socket must be owner-only"
    );

    agent.stop().await;
    Ok(())
}

// endregion: --- Directory gate

// region:    --- Absent daemon

#[test]
fn test_agent_security_reports_not_running_for_missing_and_dead_sockets() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir)?;
    let socket_path = data_dir.join("agent.sock");

    // -- Check: a socket that was never bound
    assert!(!client::responds(&socket_path));
    assert_eq!(client::status(&socket_path)?, None);
    assert!(matches!(
        client::keys(&socket_path),
        Err(SshAgentError::NotRunning(_))
    ));
    assert!(matches!(
        client::stop(&socket_path),
        Err(SshAgentError::NotRunning(_))
    ));

    // -- Setup & Fixtures: a socket whose daemon stopped without unlinking it
    let listener = std::os::unix::net::UnixListener::bind(&socket_path)?;
    drop(listener);
    assert!(
        socket_path.exists(),
        "the dead socket file must still be present"
    );

    // -- Check: a dead socket is not a running daemon
    assert!(!client::responds(&socket_path));
    assert_eq!(client::status(&socket_path)?, None);
    assert!(matches!(
        client::keys(&socket_path),
        Err(SshAgentError::NotRunning(_))
    ));
    assert!(
        matches!(
            client::stop(&socket_path),
            Err(SshAgentError::NotRunning(_))
        ),
        "stop must not report success on a dead socket"
    );
    Ok(())
}

// endregion: --- Absent daemon

// region:    --- Log capture

/// Redirects the process stderr into a temporary file for the duration of a test.
///
/// The agent logs with `eprintln!`, which writes to file descriptor 2. The test
/// process is the daemon here, so replacing descriptor 2 is the only way to read
/// what the daemon wrote. Every test that uses this guard is `serial`, and the
/// runtime is single-threaded, so no other task writes into the capture.
struct StderrCapture {
    path: PathBuf,
    saved: RawFd,
}

impl StderrCapture {
    fn start() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("bogita-agent-{}.log", Uuid::new_v4()));
        let file = std::fs::File::create(&path)?;

        // SAFETY: `dup` takes one open descriptor and returns a new one. Both
        // calls below run on the test thread with no other thread logging.
        let saved = unsafe { libc::dup(libc::STDERR_FILENO) };
        if saved < 0 {
            return Err(format!(
                "cannot duplicate stderr: {}",
                std::io::Error::last_os_error()
            )
            .into());
        }
        // SAFETY: `dup2` re-points `STDERR_FILENO` at an open descriptor and
        // leaves `file` itself open until it drops below.
        if unsafe { libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
            return Err(format!(
                "cannot redirect stderr: {}",
                std::io::Error::last_os_error()
            )
            .into());
        }
        drop(file);
        Ok(Self { path, saved })
    }

    /// Write one line straight to file descriptor 2.
    ///
    /// `eprintln!` cannot carry a marker into the capture: the harness holds the
    /// print-family output in its own thread-local sink, so those bytes never
    /// reach the descriptor `start` replaced. A raw `write` on descriptor 2 skips
    /// that sink and lands in the capture file under both runners.
    fn write_marker(&self, text: &str) -> Result<()> {
        let mut line = String::from(text);
        line.push('\n');
        let bytes = line.as_bytes();
        // SAFETY: `bytes` is live for the call and `len` is its exact length.
        let written =
            unsafe { libc::write(libc::STDERR_FILENO, bytes.as_ptr().cast(), bytes.len()) };
        if written < 0 {
            return Err(format!(
                "cannot write the capture marker: {}",
                std::io::Error::last_os_error()
            )
            .into());
        }
        if written as usize != bytes.len() {
            return Err(format!(
                "short capture marker write: {written} of {} bytes",
                bytes.len()
            )
            .into());
        }
        Ok(())
    }

    /// Restore stderr and return everything the daemon logged.
    fn finish(self) -> Result<String> {
        let _ = std::io::stderr().flush();

        // SAFETY: `self.saved` is the descriptor `start` duplicated, still open.
        if unsafe { libc::dup2(self.saved, libc::STDERR_FILENO) } < 0 {
            return Err(
                format!("cannot restore stderr: {}", std::io::Error::last_os_error()).into(),
            );
        }
        // SAFETY: the duplicate is no longer needed once stderr points at it again.
        unsafe { libc::close(self.saved) };
        Ok(std::fs::read_to_string(&self.path)?)
    }
}

/// Forty base64 characters from the middle of the private key body.
fn pem_middle_chunk(pem: &str) -> String {
    let body: String = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    body.chars().skip(40).take(40).collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// endregion: --- Log capture

// region:    --- No key material in logs

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn test_agent_security_logs_no_key_material() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(dir.path());
    let material = SshKeyMaterial::keygen_ed25519("logged@example.com")?;
    let middle = pem_middle_chunk(material.private_pem());
    assert_eq!(middle.len(), 40, "the PEM chunk must be distinctive");

    let capture = StderrCapture::start()?;
    capture.write_marker(CAPTURE_MARKER)?;
    let ring = ring_with(vec![ssh_entry("github", &material)]);
    let agent = RunningAgent::start(ring, socket_path).await?;

    // -- Exec: a full list + sign cycle
    let mut client = agent.client().await?;
    let listed = client.handle(Request::RequestIdentities).await?;
    let signed = client
        .handle(Request::SignRequest(SignRequest {
            credential: PublicCredential::Key(
                PublicKey::from_openssh(material.public_line())?
                    .key_data()
                    .clone(),
            ),
            data: DATA.to_vec(),
            flags: 0,
        }))
        .await?;
    agent.stop().await;

    // -- Check: the cycle really happened
    assert!(matches!(listed, Response::IdentitiesAnswer(_)));
    let Response::SignResponse(signature) = signed else {
        return Err(format!("expected a sign response, got {signed:?}").into());
    };
    let signature_bytes = hex(signature.as_bytes());

    // -- Check: the capture itself works
    let log = capture.finish()?;
    assert!(
        log.contains(CAPTURE_MARKER),
        "the stderr capture must observe writes, captured:\n{log}"
    );

    // -- Check: no key material, signature bytes, or field values
    assert!(
        !log.contains(&middle),
        "stderr must not carry private key body: {log}"
    );
    assert!(
        !log.contains("OPENSSH PRIVATE KEY"),
        "stderr must not carry the key armor header: {log}"
    );
    assert!(
        !log.contains(&signature_bytes),
        "stderr must not carry signature bytes: {log}"
    );
    assert!(
        !log.contains(SECRET_FIELD_VALUE),
        "stderr must not carry field values: {log}"
    );
    assert!(
        !log.contains(material.comment()),
        "stderr must not carry the advertised comment: {log}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn test_agent_security_reports_a_skipped_key_by_entry_name_only() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(dir.path());
    let material = SshKeyMaterial::keygen_ed25519("skipped@example.com")?;
    let mut broken = ssh_entry("broken", &material);
    for field in broken.fields.iter_mut() {
        if field.field_type == FieldType::SshPrivateKey {
            field.value = FieldValue::Hidden("s5-broken-pem-value".to_string());
        }
    }
    let good = ssh_entry("github", &material);

    // -- Exec
    let capture = StderrCapture::start()?;
    let ring = ring_with(vec![broken, good]);
    let agent = RunningAgent::start(ring, socket_path).await?;
    let mut client = agent.client().await?;
    let listed = client.handle(Request::RequestIdentities).await?;
    agent.stop().await;
    let log = capture.finish()?;

    // -- Check: the warning names the entry and nothing else
    assert!(
        log.contains("entry 'broken'"),
        "a skipped key must be reported by entry name, captured:\n{log}"
    );
    assert!(
        !log.contains("s5-broken-pem-value"),
        "the skipped key material must not be logged: {log}"
    );
    let Response::IdentitiesAnswer(identities) = listed else {
        return Err(format!("expected identities answer, got {listed:?}").into());
    };
    assert_eq!(identities.len(), 1, "the parsable key must still be served");
    Ok(())
}

// endregion: --- No key material in logs
