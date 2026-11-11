//! SSH agent lifecycle command handlers (start, stop, status, keys).

use bogita_core::agent::client::{self, ServedKey};
use bogita_core::agent::error::SshAgentError;
use bogita_core::agent::socket::{default_socket_path, ensure_serving_dir};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

// region:    --- Constants

/// How long `agent start` waits for the daemon child to open its socket.
const STARTUP_LIMIT: Duration = Duration::from_secs(2);

/// Pause between two connection attempts during startup.
const STARTUP_INTERVAL: Duration = Duration::from_millis(10);

// endregion: --- Constants

// region:    --- Commands

/// `bogita agent start [--socket <path>]`
///
/// Spawns the detached daemon and returns the socket path the caller must print
/// as `SSH_AUTH_SOCK`. The daemon boots locked and gets its identity from the
/// unlock request, so the start itself needs no credential.
///
/// The directory the daemon binds in is prepared here, before the spawn: a
/// pre-existing mode another user can reach is narrowed to 0700 and verified, so the
/// child's bind gate passes on the normal path. A directory whose mode stays open is
/// refused with that gate's message, which reaches the user verbatim instead of the
/// 2s startup timeout that hid the cause.
pub fn handle_start(socket: Option<String>) -> Result<PathBuf, SshAgentError> {
    let socket_path = socket_path(socket);
    if client::responds(&socket_path) {
        return Err(SshAgentError::AlreadyServing(
            socket_path.display().to_string(),
        ));
    }
    ensure_socket_dir(&socket_path)?;

    let exe = std::env::current_exe().map_err(|e| SshAgentError::Spawn(e.to_string()))?;
    daemon_command(&exe, &socket_path)
        .spawn()
        .map_err(|e| SshAgentError::Spawn(e.to_string()))?;

    wait_until_serving(&socket_path)?;
    Ok(socket_path)
}

/// `bogita agent stop`
///
/// Returns once the daemon answered SUCCESS to the stop extension.
pub fn handle_stop(socket: Option<String>) -> Result<(), SshAgentError> {
    client::stop(&socket_path(socket))
}

/// `bogita agent status`
///
/// Returns the one status line.
pub fn handle_status(socket: Option<String>) -> Result<String, SshAgentError> {
    let socket_path = socket_path(socket);
    let payload = client::status(&socket_path)?;
    Ok(format_status_line(&socket_path, payload.as_deref()))
}

/// `bogita agent keys`
///
/// Returns one line per served key.
pub fn handle_keys(socket: Option<String>) -> Result<Vec<String>, SshAgentError> {
    let keys = client::keys(&socket_path(socket))?;
    Ok(keys.iter().map(format_key_line).collect())
}

// endregion: --- Commands

// region:    --- Formatting

/// Resolve `--socket <path>`, or the derived default.
pub fn socket_path(socket: Option<String>) -> PathBuf {
    socket
        .map(PathBuf::from)
        .unwrap_or_else(default_socket_path)
}

/// Status line: `"<path>: running, <N> keys, <locked|unlocked>"` or
/// `"<path>: not running"`.
pub fn format_status_line(socket_path: &Path, payload: Option<&str>) -> String {
    match payload {
        Some(payload) => format!("{}: running, {payload}", socket_path.display()),
        None => format!("{}: not running", socket_path.display()),
    }
}

/// Key line: `"<SHA256 fingerprint> <comment>"`.
pub fn format_key_line(key: &ServedKey) -> String {
    format!("{} {}", key.fingerprint, key.comment)
}

// endregion: --- Formatting

// region:    --- Support

/// Prepare the directory `socket_path` binds in.
///
/// An explicit `--socket <path>` gets the same treatment as the derived default: the
/// operator override chooses where the socket lives, not how private its directory is.
fn ensure_socket_dir(socket_path: &Path) -> Result<(), SshAgentError> {
    let dir = socket_path
        .parent()
        .ok_or_else(|| SshAgentError::Socket(format!("{} has no parent", socket_path.display())))?;
    ensure_serving_dir(dir)?;
    Ok(())
}

/// Build the detached daemon command for `exe`.
///
/// The child re-enters the binary with the hidden `--internal` marker, serves
/// on `socket_path`, and inherits no standard stream so the parent can exit.
fn daemon_command(exe: &Path, socket_path: &Path) -> Command {
    let mut command = Command::new(exe);
    command
        .arg("agent")
        .arg("start")
        .arg("--internal")
        .arg("--socket")
        .arg(socket_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// Poll until the daemon accepts connections on `socket_path`.
fn wait_until_serving(socket_path: &Path) -> Result<(), SshAgentError> {
    let attempts = STARTUP_LIMIT.as_millis() / STARTUP_INTERVAL.as_millis();
    for _ in 0..attempts {
        if client::responds(socket_path) {
            return Ok(());
        }
        std::thread::sleep(STARTUP_INTERVAL);
    }
    Err(SshAgentError::Spawn(format!(
        "the daemon did not open {} within {}s",
        socket_path.display(),
        STARTUP_LIMIT.as_secs()
    )))
}

// endregion: --- Support

// region:    --- Tests
#[cfg(test)]
#[path = "../../test/handlers/agent.rs"]
mod agent_test;

#[cfg(all(test, feature = "integration"))]
#[path = "../../test/handlers/agent_lifecycle.rs"]
mod agent_lifecycle_test;
// endregion: --- Tests
