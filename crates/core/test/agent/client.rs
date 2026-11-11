//! Tests for the agent control client (agent/client.rs)
//!
//! Gated on the `integration` feature: every test binds a real unix socket, and
//! the round-trip tests spawn the serve loop.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::client;
use crate::agent::error::SshAgentError;
use crate::agent::keyring::{KeyRing, SharedKeyRing};
use crate::agent::server::serve;
use crate::domain::ssh::{agent_servable_field, SshKeyMaterial};
use crate::domain::Entry;
use crate::test_support::test_entry;
use signature::Verifier;
use ssh_key::public::KeyData;
use ssh_key::PublicKey;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

const DATA: &[u8] = b"client-sign-request-data";

/// Longer than the client's read timeout, and short enough to fail a park.
const BOUND: Duration = Duration::from_secs(6);

const STARTUP_LIMIT: usize = 200;

// region:    --- Fixtures

fn ssh_entry(name: &str, material: &SshKeyMaterial) -> Entry {
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

fn key_data(material: &SshKeyMaterial) -> Result<KeyData> {
    Ok(PublicKey::from_openssh(material.public_line())?
        .key_data()
        .clone())
}

async fn wait_for_socket(path: &Path) -> Result<()> {
    for _ in 0..STARTUP_LIMIT {
        if path.exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!("agent socket {} never appeared", path.display()).into())
}

// endregion: --- Fixtures

// region:    --- Bounded exchange

#[test]
fn test_agent_client_lock_gives_up_on_a_daemon_that_never_answers() -> Result<()> {
    // -- Setup & Fixtures: a listener that accepts the connection and stays silent
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(dir.path());
    std::fs::create_dir_all(
        socket_path
            .parent()
            .ok_or("the socket path has no parent")?,
    )?;
    let listener = UnixListener::bind(&socket_path)?;
    thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            thread::sleep(BOUND + Duration::from_secs(2));
            drop(stream);
        }
    });

    // -- Exec
    let started = Instant::now();
    let result = client::lock(&socket_path);

    // -- Check: the read timeout ends the exchange instead of parking the caller
    let elapsed = started.elapsed();
    assert!(
        result.is_err(),
        "a silent daemon must not answer a lock request"
    );
    assert!(
        elapsed < BOUND,
        "the exchange must be bounded by the read timeout, took {elapsed:?}"
    );
    Ok(())
}

// endregion: --- Bounded exchange

// region:    --- Identities and signatures

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_agent_client_identities_and_sign_round_trip() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("client@example.com")?;
    let socket_path = temp_socket(dir.path());
    let server = tokio::spawn(serve(
        ring_with(&[("github", &material)]),
        socket_path.clone(),
    ));
    wait_for_socket(&socket_path).await?;

    // -- Exec
    let served = client::identities(&socket_path)?;

    // -- Check: one served key, the one the ring was loaded from
    assert_eq!(served.len(), 1);
    assert_eq!(served[0].key_data(), &key_data(&material)?);
    assert_eq!(served[0].comment(), material.comment());

    // -- Exec: sign over the advertised key
    let signature = client::sign(&socket_path, &key_data(&material)?, DATA, 0)?;

    // -- Check: the signature verifies under the served key
    let public_key = PublicKey::from_openssh(material.public_line())?;
    Verifier::verify(&public_key, DATA, &signature)?;

    server.abort();
    let _ = server.await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_agent_client_sign_with_an_unserved_key_is_unknown_key() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let served = SshKeyMaterial::keygen_ed25519("served@example.com")?;
    let other = SshKeyMaterial::keygen_ed25519("other@example.com")?;
    let socket_path = temp_socket(dir.path());
    let server = tokio::spawn(serve(
        ring_with(&[("github", &served)]),
        socket_path.clone(),
    ));
    wait_for_socket(&socket_path).await?;

    // -- Exec
    let result = client::sign(&socket_path, &key_data(&other)?, DATA, 0);

    // -- Check: FAILURE(5) is the typed unknown-key refusal
    assert!(
        matches!(result, Err(SshAgentError::UnknownKey)),
        "a key the ring does not hold is an unknown key"
    );

    server.abort();
    let _ = server.await;
    Ok(())
}

// endregion: --- Identities and signatures
