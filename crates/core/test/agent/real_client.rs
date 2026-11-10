//! Real-client tests: OpenSSH `ssh-add` against the agent socket (real_client.rs)
//!
//! Gated on the `integration` feature: these tests bind a real unix socket and run
//! the real `ssh-add` binary against it.
//!
//! Feasibility, decided at spec time: `ssh-add -l` and `ssh-add -D` speak to any
//! agent through `SSH_AUTH_SOCK` on macOS and Linux, so they are the real-client
//! channel. `ssh-keysign` and `ssh -Y` are infeasible here and stay out of scope:
//! `ssh-keysign` needs a host `ssh-keysign` configuration file and a setuid binary,
//! and `ssh -Y` needs a live SSH server with X11 forwarding enabled. Neither exists
//! in the dev shell, and neither would add coverage the protocol client does not
//! already give.
//!
//! The daemon refuses every write command by design (REMOVE_ALL_IDENTITIES(19) among
//! them), so `ssh-add -D` must fail while the advertised keys stay served. That is
//! the behaviour under test, not a defect.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::client;
use crate::agent::error::SshAgentError;
use crate::agent::keyring::{KeyRing, SharedKeyRing};
use crate::agent::server::serve;
use crate::domain::ssh::{agent_servable_field, SshKeyMaterial};
use crate::test_support::test_entry;
use ssh_agent_lib::agent::Session;
use ssh_agent_lib::proto::{Request, Response};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

const STARTUP_LIMIT: usize = 200;

// region:    --- Fixtures

fn ssh_entry(name: &str, material: &SshKeyMaterial) -> crate::domain::Entry {
    let mut fields = material.to_fields();
    fields.push(agent_servable_field(true, fields.len() as i32));
    test_entry(Uuid::new_v4(), name, fields)
}

fn ring_with(materials: &[(&str, &SshKeyMaterial)]) -> SharedKeyRing {
    let mut ring = KeyRing::new();
    ring.load_entries(
        materials
            .iter()
            .map(|(name, material)| ssh_entry(name, material))
            .collect::<Vec<_>>(),
    );
    Arc::new(Mutex::new(ring))
}

fn temp_socket(dir: &Path) -> PathBuf {
    dir.join("data").join("agent.sock")
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
    handle: tokio::task::JoinHandle<std::result::Result<(), SshAgentError>>,
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

    async fn stop(self) {
        self.handle.abort();
        let _ = self.handle.await;
    }
}

/// Run `ssh-add` against the daemon's socket.
fn ssh_add(args: &[&str], socket_path: &Path) -> Result<Output> {
    Ok(Command::new("ssh-add")
        .args(args)
        .env("SSH_AUTH_SOCK", socket_path)
        .env_remove("SSH_AGENT_PID")
        .output()?)
}

fn stdout_lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect()
}

// endregion: --- Fixtures

// region:    --- ssh-add -l (criterion 4)

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_agent_real_client_ssh_add_lists_every_key_with_its_comment() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let first = SshKeyMaterial::keygen_ed25519("first@example.com")?;
    let second = SshKeyMaterial::keygen_ed25519("second@example.com")?;
    let agent = RunningAgent::start(
        ring_with(&[("first", &first), ("second", &second)]),
        temp_socket(dir.path()),
    )
    .await?;

    // -- Exec
    let listed = ssh_add(&["-l"], &agent.socket_path)?;

    // -- Check: exit 0 and exactly one line per served key
    assert!(
        listed.status.success(),
        "ssh-add -l must succeed, stderr: {}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let lines = stdout_lines(&listed);
    assert_eq!(
        lines.len(),
        2,
        "one line per served key, got {lines:#?} (stderr: {})",
        String::from_utf8_lossy(&listed.stderr)
    );

    // -- Check: each line carries its own key's comment
    for material in [&first, &second] {
        let line = lines
            .iter()
            .find(|line| line.contains(material.comment()))
            .ok_or_else(|| {
                format!(
                    "no ssh-add line carries {:?}: {lines:#?}",
                    material.comment()
                )
            })?;
        assert!(
            line.starts_with("256 SHA256:"),
            "an ed25519 line carries the key size and the SHA256 fingerprint, got {line}"
        );
    }

    agent.stop().await;
    Ok(())
}

// endregion: --- ssh-add -l (criterion 4)

// region:    --- ssh-add -D (criterion 5)

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_agent_real_client_ssh_add_cannot_remove_keys() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let first = SshKeyMaterial::keygen_ed25519("first@example.com")?;
    let second = SshKeyMaterial::keygen_ed25519("second@example.com")?;
    let agent = RunningAgent::start(
        ring_with(&[("first", &first), ("second", &second)]),
        temp_socket(dir.path()),
    )
    .await?;
    let before = ssh_add(&["-l"], &agent.socket_path)?;
    assert!(before.status.success());

    // -- Exec: the daemon refuses REMOVE_ALL_IDENTITIES(19) by design
    let removed = ssh_add(&["-D"], &agent.socket_path)?;

    // -- Check: the removal is refused
    assert!(
        !removed.status.success(),
        "ssh-add -D must fail against an agent that refuses removal, stdout: {}",
        String::from_utf8_lossy(&removed.stdout)
    );

    // -- Exec: list again
    let after = ssh_add(&["-l"], &agent.socket_path)?;

    // -- Check: the ring is unchanged
    assert!(after.status.success(), "ssh-add -l must still succeed");
    let mut before_lines = stdout_lines(&before);
    let mut after_lines = stdout_lines(&after);
    before_lines.sort();
    after_lines.sort();
    assert_eq!(
        after_lines, before_lines,
        "a refused ssh-add -D must leave the served keys untouched"
    );

    // -- Check: the protocol view agrees
    let mut client = ssh_agent_lib::client::Client::new(
        tokio::net::UnixStream::connect(&agent.socket_path).await?,
    );
    let listed = client.handle(Request::RequestIdentities).await?;
    let Response::IdentitiesAnswer(identities) = listed else {
        return Err(format!("expected identities answer, got {listed:?}").into());
    };
    assert_eq!(identities.len(), 2);
    assert_eq!(client::keys(&agent.socket_path)?.len(), 2);

    agent.stop().await;
    Ok(())
}

// endregion: --- ssh-add -D (criterion 5)
