//! Integration tests for the agent control channel (agent/control.rs, agent/client.rs)
//!
//! Gated on the `integration` feature: these tests bind a real unix socket, spawn
//! the serve loop, and connect to it.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use super::security_support::FixedSource;
use crate::agent::client;
use crate::agent::control::GRACEFUL_DRAIN;
use crate::agent::error::SshAgentError;
use crate::agent::keyring::KeyRing;
use crate::agent::server::{serve, serve_with_lock, serve_with_signals, SharedKeyRing};
use crate::agent::unlock::VaultLock;
use crate::domain::ssh::{agent_servable_field, SshKeyMaterial};
use crate::domain::{AgeIdentity, Entry, SecretString};
use crate::ports::EncryptedIdentityFile;
use crate::storage::identity::write_identity_encrypted;
use crate::test_support::test_entry;
use ssh_agent_lib::agent::Session;
use ssh_agent_lib::proto::{Extension, Request, Response};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

const STARTUP_LIMIT: usize = 200;

// region:    --- Fixtures

fn ring_with(materials: &[(&str, &SshKeyMaterial)]) -> SharedKeyRing {
    let mut ring = KeyRing::new();
    ring.load_entries(
        materials
            .iter()
            .map(|(name, material)| {
                let mut fields = material.to_fields();
                let idx = fields.len() as i32;
                fields.push(agent_servable_field(true, idx));
                test_entry(Uuid::new_v4(), name, fields)
            })
            .collect::<Vec<_>>(),
    );
    Arc::new(Mutex::new(ring))
}

fn locked_ring() -> SharedKeyRing {
    Arc::new(Mutex::new(KeyRing::new()))
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

fn temp_socket(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("data").join("agent.sock")
}

/// Block until nothing is bound at `path` (or the budget runs out).
async fn wait_for_absent(path: &Path) -> Result<()> {
    for _ in 0..STARTUP_LIMIT {
        if !path.exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!("agent socket {} was never unlinked", path.display()).into())
}

fn unknown_extension() -> Extension {
    Extension {
        name: "bogita-keys@bogita".to_string(),
        details: Vec::new().into(),
    }
}

/// One servable ed25519 entry: the ring caches it only with the opt-in gate set.
fn servable_entries(material: &SshKeyMaterial) -> Vec<Entry> {
    let mut fields = material.to_fields();
    let idx = fields.len() as i32;
    fields.push(agent_servable_field(true, idx));
    vec![test_entry(Uuid::new_v4(), "rpc", fields)]
}

// endregion: --- Fixtures

// region:    --- Absent daemon

#[test]
fn test_agent_control_reports_absent_daemon() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);

    // -- Check
    assert!(
        !client::responds(&socket_path),
        "no process may answer on a socket that was never bound"
    );
    assert_eq!(client::status(&socket_path)?, None);
    assert!(matches!(
        client::stop(&socket_path),
        Err(SshAgentError::NotRunning(_))
    ));
    assert!(matches!(
        client::keys(&socket_path),
        Err(SshAgentError::NotRunning(_))
    ));
    Ok(())
}

// endregion: --- Absent daemon

// region:    --- Status

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_agent_control_status_reports_key_count_and_state() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);
    let material = SshKeyMaterial::keygen_ed25519("status@example.com")?;
    let server = tokio::spawn(serve(
        ring_with(&[("github", &material)]),
        socket_path.clone(),
    ));
    wait_for_socket(&socket_path).await?;

    // -- Exec
    let payload = client::status(&socket_path)?;

    // -- Check
    assert_eq!(payload.as_deref(), Some("1 keys, unlocked"));

    client::stop(&socket_path)?;
    let _ = tokio::time::timeout(Duration::from_secs(6), server).await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_agent_control_status_reports_locked_ring() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);
    let server = tokio::spawn(serve(locked_ring(), socket_path.clone()));
    wait_for_socket(&socket_path).await?;

    // -- Exec
    let payload = client::status(&socket_path)?;

    // -- Check
    assert_eq!(payload.as_deref(), Some("0 keys, locked"));

    client::stop(&socket_path)?;
    let _ = tokio::time::timeout(Duration::from_secs(6), server).await??;
    Ok(())
}

// endregion: --- Status

// region:    --- Keys

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_agent_control_keys_reports_fingerprint_and_comment() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);
    let material = SshKeyMaterial::keygen_ed25519("keys@example.com")?;
    let server = tokio::spawn(serve(
        ring_with(&[("github", &material)]),
        socket_path.clone(),
    ));
    wait_for_socket(&socket_path).await?;

    // -- Exec
    let keys = client::keys(&socket_path)?;

    // -- Check
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].comment, "keys@example.com");
    assert_eq!(keys[0].fingerprint, material.fingerprint());

    client::stop(&socket_path)?;
    let _ = tokio::time::timeout(Duration::from_secs(6), server).await??;
    Ok(())
}

// endregion: --- Keys

// region:    --- Stop

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_agent_control_stop_exits_and_unlinks_socket() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);
    let server = tokio::spawn(serve(locked_ring(), socket_path.clone()));
    wait_for_socket(&socket_path).await?;

    // -- Exec
    client::stop(&socket_path)?;

    // -- Check
    let served = tokio::time::timeout(Duration::from_secs(6), server).await??;
    served?;
    assert!(
        !socket_path.exists(),
        "the daemon must unlink the socket file when it stops"
    );
    assert!(!client::responds(&socket_path));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_agent_control_stop_drains_within_the_limit() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);
    let server = tokio::spawn(serve(locked_ring(), socket_path.clone()));
    wait_for_socket(&socket_path).await?;
    let idle = std::os::unix::net::UnixStream::connect(&socket_path)?;

    // -- Exec
    let started = Instant::now();
    client::stop(&socket_path)?;
    let served = tokio::time::timeout(GRACEFUL_DRAIN + Duration::from_secs(1), server).await??;
    let elapsed = started.elapsed();

    // -- Check
    served?;
    assert!(
        elapsed < GRACEFUL_DRAIN + Duration::from_secs(1),
        "an open connection must not hold the daemon past the drain limit, waited {elapsed:?}"
    );
    assert!(
        elapsed >= GRACEFUL_DRAIN - Duration::from_secs(1),
        "the daemon must drain an in-flight session instead of exiting at once, waited {elapsed:?}"
    );
    drop(idle);
    Ok(())
}

// endregion: --- Stop

// region:    --- Stop then restart

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_agent_control_restart_survives_the_draining_daemon() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);
    let draining = tokio::spawn(serve(locked_ring(), socket_path.clone()));
    wait_for_socket(&socket_path).await?;
    let idle = std::os::unix::net::UnixStream::connect(&socket_path)?;

    // -- Exec
    client::stop(&socket_path)?;
    wait_for_absent(&socket_path).await?;
    let restarted = tokio::spawn(serve(locked_ring(), socket_path.clone()));
    wait_for_socket(&socket_path).await?;

    // -- Check
    let served = tokio::time::timeout(GRACEFUL_DRAIN + Duration::from_secs(1), draining).await??;
    served?;
    assert!(
        socket_path.exists(),
        "the draining daemon must not unlink the restarted daemon's socket"
    );
    assert_eq!(
        client::status(&socket_path)?,
        Some("0 keys, locked".to_string()),
        "the restarted daemon must still answer the control channel"
    );

    client::stop(&socket_path)?;
    let _ = tokio::time::timeout(Duration::from_secs(6), restarted).await??;
    drop(idle);
    Ok(())
}

// endregion: --- Stop then restart

// region:    --- Signals

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_agent_control_sigint_stops_and_unlinks_socket() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);
    let server = tokio::spawn(serve_with_signals(
        locked_ring(),
        socket_path.clone(),
        Arc::new(crate::agent::NoLock),
    ));
    wait_for_socket(&socket_path).await?;

    // -- Exec
    // SAFETY: `raise` delivers a signal to the calling process. The handler
    // `serve_with_signals` installed only records the request.
    let raised = unsafe { libc::raise(libc::SIGINT) };

    // -- Check
    assert_eq!(raised, 0, "the test process cannot raise SIGINT");
    let served = tokio::time::timeout(Duration::from_secs(6), server).await??;
    served?;
    assert!(
        !socket_path.exists(),
        "a signal must reach the stop path and unlink the socket file"
    );
    Ok(())
}

// endregion: --- Signals

// region:    --- Lock round-trip

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_agent_control_unlock_and_lock_round_trip() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);
    let material = SshKeyMaterial::keygen_ed25519("rpc@example.com")?;
    let passphrase = SecretString::from("rpc passphrase");
    let identity_path = dir.path().join("identity.age");
    write_identity_encrypted(&AgeIdentity::generate(), &passphrase, &identity_path)?;
    let ring: SharedKeyRing = Arc::new(Mutex::new(KeyRing::new()));
    let lock = Arc::new(VaultLock::new(
        identity_path,
        EncryptedIdentityFile,
        FixedSource::new(servable_entries(&material)),
        Arc::clone(&ring),
    ));
    let server = tokio::spawn(serve_with_lock(
        Arc::clone(&ring),
        socket_path.clone(),
        lock,
    ));
    wait_for_socket(&socket_path).await?;

    // -- Exec: a wrong passphrase
    let refused = client::unlock(&socket_path, &SecretString::from("wrong passphrase"));

    // -- Check: the refusal is typed and the ring stays locked
    assert!(matches!(refused, Err(SshAgentError::UnlockRefused)));
    assert_eq!(
        client::status(&socket_path)?,
        Some("0 keys, locked".to_string())
    );

    // -- Exec: the right passphrase
    client::unlock(&socket_path, &passphrase)?;
    let mut client =
        ssh_agent_lib::client::Client::new(tokio::net::UnixStream::connect(&socket_path).await?);
    let listed = client.handle(Request::RequestIdentities).await?;

    // -- Check: IDENTITIES_ANSWER advertises the vault key and the status moves on
    let Response::IdentitiesAnswer(identities) = listed else {
        return Err(format!("expected identities answer, got {listed:?}").into());
    };
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].comment, "rpc@example.com");
    assert_eq!(
        client::status(&socket_path)?,
        Some("1 keys, unlocked".to_string())
    );

    // -- Exec: lock, then ask again on the same connection
    client::lock(&socket_path)?;
    let listed_locked = client.handle(Request::RequestIdentities).await?;
    let refused_unknown = client
        .handle(Request::Extension(unknown_extension()))
        .await?;

    // -- Check: the lock evicts the ring and the wire refusals stay typed
    assert_eq!(listed_locked, Response::Failure);
    assert_eq!(refused_unknown, Response::ExtensionFailure);
    assert_eq!(
        client::status(&socket_path)?,
        Some("0 keys, locked".to_string())
    );

    client::stop(&socket_path)?;
    let _ = tokio::time::timeout(Duration::from_secs(6), server).await??;
    Ok(())
}

// endregion: --- Lock round-trip

// region:    --- Refusals

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_agent_control_refuses_unknown_extension() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);
    let server = tokio::spawn(serve(locked_ring(), socket_path.clone()));
    wait_for_socket(&socket_path).await?;
    let mut client =
        ssh_agent_lib::client::Client::new(tokio::net::UnixStream::connect(&socket_path).await?);

    // -- Exec
    let reply = client
        .handle(Request::Extension(unknown_extension()))
        .await?;

    // -- Check
    assert_eq!(reply, Response::ExtensionFailure);

    client::stop(&socket_path)?;
    let _ = tokio::time::timeout(Duration::from_secs(6), server).await??;
    Ok(())
}

// endregion: --- Refusals
