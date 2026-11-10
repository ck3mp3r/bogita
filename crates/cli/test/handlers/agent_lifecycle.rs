//! Integration tests for the agent lifecycle handlers (handlers/agent.rs)
//!
//! Gated on the `integration` feature: these tests bind a real unix socket, run
//! the serve loop, and talk to it over the socket.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::handlers::agent::{handle_keys, handle_start, handle_status, handle_stop};
use crate::handlers::entry::{handle_keygen, handle_serve};
use crate::test_support::make_registry;
use bogita_core::agent::error::SshAgentError;
use bogita_core::agent::{serve, KeyRing, SharedKeyRing};
use bogita_core::crypto::AgeCrypto;
use bogita_core::domain::{agent_servable_field, SshKeyMaterial};
use bogita_core::storage::sqlite::SqliteStorage;
use bogita_core::test_support::test_entry;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

const STARTUP_LIMIT: usize = 200;

// region:    --- Fixtures

fn ring_with(material: &SshKeyMaterial) -> SharedKeyRing {
    let mut ring = KeyRing::new();
    let mut fields = material.to_fields();
    let idx = fields.len() as i32;
    fields.push(agent_servable_field(true, idx));
    ring.load_entries(vec![test_entry(Uuid::new_v4(), "github", fields)]);
    Arc::new(Mutex::new(ring))
}

fn locked_ring() -> SharedKeyRing {
    Arc::new(Mutex::new(KeyRing::new()))
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

fn temp_socket(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("data").join("agent.sock")
}

/// The `--socket` override as the command line would pass it.
fn override_of(path: &Path) -> Option<String> {
    Some(path.display().to_string())
}

// endregion: --- Fixtures

// region:    --- Status

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_prints_one_running_line() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);
    let material = SshKeyMaterial::keygen_ed25519("cli@example.com")?;
    let server = tokio::spawn(serve(ring_with(&material), socket_path.clone()));
    wait_for_socket(&socket_path).await?;

    // -- Exec
    let line = handle_status(override_of(&socket_path))?;

    // -- Check
    assert_eq!(
        line,
        format!("{}: running, 1 keys, unlocked", socket_path.display())
    );

    handle_stop(override_of(&socket_path))?;
    let _ = tokio::time::timeout(Duration::from_secs(6), server).await??;
    Ok(())
}

#[test]
fn status_prints_not_running_without_a_daemon() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);

    // -- Exec
    let line = handle_status(override_of(&socket_path))?;

    // -- Check
    assert_eq!(line, format!("{}: not running", socket_path.display()));
    Ok(())
}

// endregion: --- Status

// region:    --- Keys

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn keys_prints_fingerprint_and_comment() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);
    let material = SshKeyMaterial::keygen_ed25519("keys@example.com")?;
    let server = tokio::spawn(serve(ring_with(&material), socket_path.clone()));
    wait_for_socket(&socket_path).await?;

    // -- Exec
    let lines = handle_keys(override_of(&socket_path))?;

    // -- Check
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0],
        format!("{} keys@example.com", material.fingerprint())
    );

    handle_stop(override_of(&socket_path))?;
    let _ = tokio::time::timeout(Duration::from_secs(6), server).await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_keys_lists_the_storage_authored_servable_key() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("cli_test.db");
    let socket_path = temp_socket(&dir);
    let (registry, identity, _vault) = make_registry(&db_path, &[]).await?;
    let keygen = handle_keygen(
        Some("serve@example.com".to_string()),
        None,
        None,
        registry.clone(),
        &identity,
    )
    .await?;
    handle_serve(keygen.name.clone(), true, None, registry.clone(), &identity).await?;

    // The daemon reloads the vault through its own pool, exactly like
    // `bogita agent start` does at startup.
    let storage = SqliteStorage::new(&db_path, AgeCrypto).await?;
    let mut ring = KeyRing::new();
    let loaded = ring.populate(&storage, &identity).await?;
    assert_eq!(
        loaded, 1,
        "the storage-authored boolean gate must load the key"
    );
    let server = tokio::spawn(serve(Arc::new(Mutex::new(ring)), socket_path.clone()));
    wait_for_socket(&socket_path).await?;

    // -- Exec
    let lines = handle_keys(override_of(&socket_path))?;

    // -- Check
    assert_eq!(
        lines,
        vec![format!("{} serve@example.com", keygen.fingerprint)],
        "agent keys must list the key the CLI marked servable"
    );

    handle_stop(override_of(&socket_path))?;
    let _ = tokio::time::timeout(Duration::from_secs(6), server).await??;
    Ok(())
}

#[test]
fn keys_without_a_daemon_report_not_running() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);

    // -- Exec
    let result = handle_keys(override_of(&socket_path));

    // -- Check
    assert!(
        matches!(result, Err(SshAgentError::NotRunning(_))),
        "keys without a daemon must report the missing agent, got {result:?}"
    );
    Ok(())
}

// endregion: --- Keys

// region:    --- Stop

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_ends_the_daemon_and_removes_the_socket() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);
    let server = tokio::spawn(serve(locked_ring(), socket_path.clone()));
    wait_for_socket(&socket_path).await?;

    // -- Exec
    handle_stop(override_of(&socket_path))?;

    // -- Check
    let served = tokio::time::timeout(Duration::from_secs(6), server).await??;
    served?;
    assert!(!socket_path.exists(), "the socket file must be unlinked");
    Ok(())
}

#[test]
fn stop_without_a_daemon_reports_not_running() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);

    // -- Exec
    let result = handle_stop(override_of(&socket_path));

    // -- Check
    assert!(
        matches!(result, Err(SshAgentError::NotRunning(_))),
        "stop without a daemon must report the missing agent, got {result:?}"
    );
    Ok(())
}

// endregion: --- Stop

// region:    --- Start

#[test]
fn start_refuses_a_file_socket_parent_without_chmod() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    // -- Setup & Fixtures: `--socket <some-file>/agent.sock`
    let dir = tempfile::tempdir()?;
    let parent = dir.path().join("data");
    std::fs::write(&parent, b"not a directory")?;
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o644))?;
    let socket_path = parent.join("agent.sock");

    // -- Exec
    let started = std::time::Instant::now();
    let result = handle_start(override_of(&socket_path));
    let elapsed = started.elapsed();

    // -- Check
    assert!(
        matches!(result, Err(SshAgentError::Socket(_))),
        "a file parent must be refused before the spawn, got {result:?}"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "the refusal is a decision, not the 2s spawn poll, took {elapsed:?}"
    );
    assert_eq!(
        std::fs::metadata(&parent)?.permissions().mode() & 0o777,
        0o644,
        "a refusal must not chmod the file the operator pointed at"
    );
    Ok(())
}

#[test]
fn start_refuses_while_an_agent_answers() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(&dir);
    std::fs::create_dir_all(socket_path.parent().expect("socket parent"))?;
    let live = std::os::unix::net::UnixListener::bind(&socket_path)?;

    // -- Exec
    let result = handle_start(override_of(&socket_path));

    // -- Check
    assert!(
        matches!(result, Err(SshAgentError::AlreadyServing(_))),
        "start must refuse a socket that already answers, got {result:?}"
    );
    drop(live);
    Ok(())
}

// endregion: --- Start
