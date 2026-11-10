//! Unit tests for the agent command handlers (handlers/agent.rs)
//!
//! No daemon: the formatting, the socket-path resolution, and the socket-directory
//! pre-check `agent start` runs before it spawns.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::handlers::agent::{
    daemon_command, ensure_socket_dir, format_key_line, format_status_line, socket_path,
};
use bogita_core::agent::client::ServedKey;
use bogita_core::agent::default_socket_path;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[test]
fn status_line_reports_a_running_agent() {
    let line = format_status_line(Path::new("/tmp/agent.sock"), Some("2 keys, unlocked"));
    assert_eq!(line, "/tmp/agent.sock: running, 2 keys, unlocked");
}

#[test]
fn status_line_reports_an_absent_agent() {
    let line = format_status_line(Path::new("/tmp/agent.sock"), None);
    assert_eq!(line, "/tmp/agent.sock: not running");
}

#[test]
fn key_line_pairs_fingerprint_and_comment() {
    let key = ServedKey {
        fingerprint: "SHA256:abc123".to_string(),
        comment: "alice@laptop".to_string(),
    };

    assert_eq!(format_key_line(&key), "SHA256:abc123 alice@laptop");
}

#[test]
fn socket_path_prefers_the_operator_override() {
    assert_eq!(
        socket_path(Some("/tmp/other.sock".to_string())),
        PathBuf::from("/tmp/other.sock")
    );
}

#[test]
fn socket_path_derives_the_default_when_unset() {
    assert_eq!(socket_path(None), default_socket_path());
}

#[test]
fn daemon_command_reenters_the_binary_with_the_internal_marker() {
    let command = daemon_command(Path::new("/opt/bogita"), Path::new("/tmp/agent.sock"));
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    assert_eq!(
        command.get_program().to_string_lossy(),
        "/opt/bogita",
        "the child is the running binary"
    );
    assert_eq!(
        args,
        [
            "agent",
            "start",
            "--internal",
            "--socket",
            "/tmp/agent.sock"
        ],
        "the child re-enters with the hidden marker and the resolved socket"
    );
}

// region:    --- Socket directory pre-check

/// Mode of `path`, with the file-type bits cleared.
fn mode_of(path: &Path) -> Result<u32> {
    Ok(std::fs::metadata(path)?.permissions().mode() & 0o777)
}

#[test]
fn start_dir_precheck_narrows_an_open_socket_dir() -> Result<()> {
    // -- Setup & Fixtures
    let sandbox = tempfile::tempdir()?;
    let data_dir = sandbox.path().join("data");
    std::fs::create_dir(&data_dir)?;
    std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o755))?;
    let socket = data_dir.join("agent.sock");

    // -- Exec
    ensure_socket_dir(&socket)?;

    // -- Check
    assert_eq!(
        mode_of(&data_dir)?,
        0o700,
        "start must heal the data dir before it spawns, so the child never refuses it"
    );
    Ok(())
}

#[test]
fn start_dir_precheck_creates_a_missing_socket_dir() -> Result<()> {
    // -- Setup & Fixtures
    let sandbox = tempfile::tempdir()?;
    let socket = sandbox.path().join("data").join("agent.sock");

    // -- Exec
    ensure_socket_dir(&socket)?;

    // -- Check
    assert_eq!(mode_of(&sandbox.path().join("data"))?, 0o700);
    Ok(())
}

#[test]
fn start_dir_precheck_leaves_an_owner_only_socket_dir_alone() -> Result<()> {
    // -- Setup & Fixtures
    let sandbox = tempfile::tempdir()?;
    let data_dir = sandbox.path().join("data");
    std::fs::create_dir(&data_dir)?;
    std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700))?;
    let socket = data_dir.join("agent.sock");

    // -- Exec
    ensure_socket_dir(&socket)?;

    // -- Check
    assert_eq!(mode_of(&data_dir)?, 0o700);
    Ok(())
}

// endregion: --- Socket directory pre-check
