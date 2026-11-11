//! Shared fixtures for the agent security tests (agent/socket.rs, agent/server.rs)
//!
//! Gated on the `integration` feature by the module declarations in
//! `crates/core/src/agent/server.rs`: these tests bind real unix sockets and
//! redirect the process stderr.
//!
//! Runner note: the capture tests must behave the same under `cargo test` and under
//! a per-test-process runner. The libtest harness arms a thread-local output sink
//! (`std::io::set_output_capture`) on the test thread and on every thread that
//! thread spawns, so `println!`/`eprintln!` output is routed into that sink and
//! never reaches file descriptor 2 — the descriptor `StderrCapture` replaces. A
//! raw `libc::write` on descriptor 2 bypasses the sink and lands in the capture
//! file, so the marker goes through `StderrCapture::write_marker`. A runner that
//! starts one process per test arms no sink, so the same call works there too. For
//! the same reason the daemon writes the warning the log tests assert on with
//! `std::io::stderr()` (`KeyRing`'s `skip`) instead of `eprintln!`: print-macro
//! output is what the sink swallows.

pub(crate) type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::error::SshAgentError;
use crate::agent::keyring::{KeyRing, SharedKeyRing};
use crate::agent::server::{serve, serve_with_lock};
use crate::agent::source::UnlockSource;
use crate::agent::unlock::VaultService;
use crate::domain::ssh::{agent_servable_field, SshKeyMaterial};
use crate::domain::{AgeIdentity, Entry, FieldType, FieldValue, Vault};
use crate::test_support::{test_entry, test_field};
use crate::vault::registry::VaultAccess;
use async_trait::async_trait;
use ssh_agent_lib::client::Client;
use std::future::Future;
use std::io::Write;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use uuid::Uuid;

pub(crate) const DATA: &[u8] = b"ssh-agent-sign-request-data";
const STARTUP_LIMIT: usize = 200;

/// Value of the plaintext field that must never reach a log line.
pub(crate) const SECRET_FIELD_KEY: &str = "recovery_note";
pub(crate) const SECRET_FIELD_VALUE: &str = "s5-field-value-do-not-log";

/// A line the test itself writes while the capture is active.
pub(crate) const CAPTURE_MARKER: &str = "s5-capture-marker";

// region:    --- Fixtures

pub(crate) fn ssh_entry(name: &str, material: &SshKeyMaterial) -> Entry {
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

pub(crate) fn locked_ring() -> SharedKeyRing {
    Arc::new(Mutex::new(KeyRing::new()))
}

pub(crate) fn ring_with(entries: Vec<Entry>) -> SharedKeyRing {
    let mut ring = KeyRing::new();
    ring.load_entries(entries);
    Arc::new(Mutex::new(ring))
}

pub(crate) fn temp_socket(dir: &Path) -> PathBuf {
    dir.join("data").join("agent.sock")
}

pub(crate) fn mode_of(path: &Path) -> Result<u32> {
    Ok(std::fs::metadata(path)?.permissions().mode() & 0o777)
}

/// Identity source that ignores the identity and returns fixed entries.
///
/// The registry half answers as an empty store: these tests drive the ring only,
/// through `entries`.
pub(crate) struct FixedSource(Vec<Entry>);

impl FixedSource {
    pub(crate) fn new(entries: Vec<Entry>) -> Self {
        Self(entries)
    }
}

#[async_trait]
impl UnlockSource for FixedSource {
    async fn entries(&self, _identity: &AgeIdentity) -> crate::error::Result<Vec<Entry>> {
        Ok(self.0.clone())
    }
}

#[async_trait]
impl VaultAccess for FixedSource {
    async fn list_vaults(&self) -> crate::error::Result<Vec<Vault>> {
        Ok(Vec::new())
    }

    async fn add_vault(&self, _vault: &Vault) -> crate::error::Result<()> {
        Ok(())
    }

    async fn remove_vault(&self, _id: Uuid) -> crate::error::Result<()> {
        Ok(())
    }

    async fn set_default(&self, _id: Uuid) -> crate::error::Result<()> {
        Ok(())
    }

    async fn list_entries(
        &self,
        _vault_id: Uuid,
        _query: Option<&str>,
        _identity: &AgeIdentity,
    ) -> crate::error::Result<Vec<Entry>> {
        Ok(Vec::new())
    }

    async fn get_entry(
        &self,
        _id: Uuid,
        _identity: &AgeIdentity,
    ) -> crate::error::Result<Option<Entry>> {
        Ok(None)
    }

    async fn save_entry(
        &self,
        _entry: &Entry,
        _identity: &AgeIdentity,
    ) -> crate::error::Result<()> {
        Ok(())
    }

    async fn delete_entry(&self, _id: Uuid) -> crate::error::Result<()> {
        Ok(())
    }
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

pub(crate) struct RunningAgent {
    handle: JoinHandle<std::result::Result<(), SshAgentError>>,
    socket_path: PathBuf,
}

impl RunningAgent {
    pub(crate) async fn start(ring: SharedKeyRing, socket_path: PathBuf) -> Result<Self> {
        Self::spawn(serve(ring, socket_path.clone()), socket_path).await
    }

    pub(crate) async fn start_with_lock<L: VaultService + 'static>(
        ring: SharedKeyRing,
        socket_path: PathBuf,
        lock: Arc<L>,
    ) -> Result<Self> {
        Self::spawn(
            serve_with_lock(ring, socket_path.clone(), lock),
            socket_path,
        )
        .await
    }

    async fn spawn(
        served: impl Future<Output = std::result::Result<(), SshAgentError>> + Send + 'static,
        socket_path: PathBuf,
    ) -> Result<Self> {
        let handle = tokio::spawn(served);
        wait_for_socket(&socket_path).await?;
        Ok(Self {
            handle,
            socket_path,
        })
    }

    pub(crate) async fn client(&self) -> Result<Client<UnixStream>> {
        Ok(Client::new(UnixStream::connect(&self.socket_path).await?))
    }

    pub(crate) async fn stop(self) {
        self.handle.abort();
        let _ = self.handle.await;
    }
}

// endregion: --- Fixtures

// region:    --- Log capture

/// Redirects the process stderr into a temporary file for the duration of a test.
///
/// The agent logs with `eprintln!`, which writes to file descriptor 2. The test
/// process is the daemon here, so replacing descriptor 2 is the only way to read
/// what the daemon wrote. Every test that uses this guard is `serial`, and the
/// runtime is single-threaded, so no other task writes into the capture.
pub(crate) struct StderrCapture {
    path: PathBuf,
    saved: RawFd,
}

impl StderrCapture {
    pub(crate) fn start() -> Result<Self> {
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
    pub(crate) fn write_marker(&self, text: &str) -> Result<()> {
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
    pub(crate) fn finish(self) -> Result<String> {
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
pub(crate) fn pem_middle_chunk(pem: &str) -> String {
    let body: String = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    body.chars().skip(40).take(40).collect()
}

/// Forty bech32 characters from the middle of the age secret key, past the prefix.
pub(crate) fn identity_middle_chunk(secret: &str) -> String {
    secret.chars().skip(20).take(40).collect()
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// endregion: --- Log capture
