//! Socket gate and daemon-liveness guards for the agent daemon (agent/socket.rs)
//!
//! Covered here:
//! - the socket directory gate fails closed: a pre-existing directory that is not
//!   0700 refuses the start instead of being widened into safety
//! - the client reports a missing or dead socket as "not running" and never hangs
//!
//! Out of scope: process exit codes. `main.rs` maps a `SshAgentError::Socket` from
//! `serve` onto `exit_with_error` (stderr + exit 1); this file asserts the error at
//! the `serve` boundary, which is the value that path consumes.

use super::security_support::{locked_ring, mode_of, Result, RunningAgent};

use crate::agent::client;
use crate::agent::error::SshAgentError;
use crate::agent::server::serve;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

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
