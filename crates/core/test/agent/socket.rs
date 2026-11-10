//! Tests for agent socket path derivation, the directory heal, and the peer gate
//! (agent/socket.rs)
//!
//! No socket is bound. The heal tests write real directories: the narrowing is a
//! `chmod` and the verification is a fresh `stat`, so a real filesystem is the only
//! honest fixture.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::error::SshAgentError;
use crate::agent::socket::{
    default_socket_path, ensure_serving_dir, heal_verdict, narrowing_notice, peer_matches, DirHeal,
    DIR_MODE, SOCKET_FILE_NAME, SOCKET_MODE,
};
use crate::storage::config::default_data_dir;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
fn test_agent_socket_path_ends_with_socket_file_name() {
    // -- Exec
    let path = default_socket_path();

    // -- Check
    assert_eq!(
        path.file_name().map(|n| n.to_string_lossy().to_string()),
        Some(SOCKET_FILE_NAME.to_string())
    );
}

#[test]
fn test_agent_socket_path_lives_in_the_data_dir() {
    // -- Exec
    let path = default_socket_path();

    // -- Check
    assert_eq!(
        path.parent().map(|p| p.to_path_buf()),
        Some(default_data_dir())
    );
}

#[test]
fn test_agent_socket_path_follows_the_app_name_gate() {
    // -- Setup & Fixtures
    let expected = if cfg!(debug_assertions) {
        "bogita-dev"
    } else {
        "bogita"
    };

    // -- Exec
    let path = default_socket_path();

    // -- Check
    assert_eq!(
        path.parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string()),
        Some(expected.to_string())
    );
}

#[test]
fn test_agent_socket_modes_are_owner_only() {
    // -- Check
    assert_eq!(DIR_MODE, 0o700);
    assert_eq!(SOCKET_MODE, 0o600);
}

#[test]
fn test_agent_socket_peer_matches_same_uid() {
    // -- Check
    assert!(peer_matches(501, 501));
}

#[test]
fn test_agent_socket_peer_matches_rejects_other_uid() {
    // -- Check
    assert!(!peer_matches(501, 502));
}

// region:    --- Directory heal

/// Mode of `path`, with the file-type bits cleared.
fn mode_of(path: &Path) -> Result<u32> {
    Ok(std::fs::metadata(path)?.permissions().mode() & 0o777)
}

/// A directory that another user can reach — the mode an older bogita created.
fn open_dir(parent: &Path) -> Result<std::path::PathBuf> {
    let dir = parent.join("data");
    std::fs::create_dir(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))?;
    Ok(dir)
}

#[test]
fn test_agent_socket_heal_narrows_an_open_dir_to_owner_only() -> Result<()> {
    // -- Setup & Fixtures
    let sandbox = tempfile::tempdir()?;
    let dir = open_dir(sandbox.path())?;

    // -- Exec
    let healed = ensure_serving_dir(&dir)?;

    // -- Check
    assert_eq!(
        healed,
        DirHeal::Narrowed { from: 0o755 },
        "a directory another user can reach must be narrowed, not refused"
    );
    assert_eq!(
        mode_of(&dir)?,
        0o700,
        "the re-stat must attest the narrowed mode"
    );
    Ok(())
}

#[test]
fn test_agent_socket_heal_creates_a_missing_dir_owner_only() -> Result<()> {
    // -- Setup & Fixtures
    let sandbox = tempfile::tempdir()?;
    let dir = sandbox.path().join("nested").join("data");

    // -- Exec
    let healed = ensure_serving_dir(&dir)?;

    // -- Check
    assert_eq!(healed, DirHeal::Created);
    assert_eq!(mode_of(&dir)?, 0o700, "the created tree is owner-only");
    Ok(())
}

#[test]
fn test_agent_socket_heal_leaves_an_owner_only_dir_alone() -> Result<()> {
    // -- Setup & Fixtures
    let sandbox = tempfile::tempdir()?;
    let dir = sandbox.path().join("data");
    std::fs::create_dir(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;

    // -- Exec
    let healed = ensure_serving_dir(&dir)?;

    // -- Check
    assert_eq!(
        healed,
        DirHeal::AlreadyPrivate,
        "an owner-only dir is not narrowed, so no narrowing notice is built"
    );
    assert_eq!(mode_of(&dir)?, 0o700);
    Ok(())
}

#[test]
fn test_agent_socket_heal_accepts_a_dir_tighter_than_owner_only() -> Result<()> {
    // -- Setup & Fixtures
    let sandbox = tempfile::tempdir()?;
    let dir = sandbox.path().join("data");
    std::fs::create_dir(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500))?;

    // -- Exec
    let healed = ensure_serving_dir(&dir)?;

    // -- Check
    assert_eq!(healed, DirHeal::AlreadyPrivate);
    assert_eq!(mode_of(&dir)?, 0o500, "a tighter mode is left untouched");
    Ok(())
}

#[test]
fn test_agent_socket_heal_refuses_when_the_restat_keeps_the_open_mode() -> Result<()> {
    // -- Setup & Fixtures
    let sandbox = tempfile::tempdir()?;
    let dir = open_dir(sandbox.path())?;

    // -- Exec
    let started = Instant::now();
    let refused = heal_verdict(&dir, 0o755, 0o755);
    let elapsed = started.elapsed();

    // -- Check
    let Err(SshAgentError::Socket(message)) = refused else {
        return Err(format!("a mode that stays open must be refused, got {refused:?}").into());
    };
    assert_eq!(
        message,
        format!(
            "{} has mode 0o755; the agent serves only from a 0o700 directory (chmod 700 '{}')",
            dir.display(),
            dir.display()
        ),
        "the refusal must carry the bind gate's message verbatim, never the 2s timeout text"
    );
    assert!(
        elapsed < Duration::from_millis(300),
        "the refusal is a decision, not a poll: it must fail in under 300 ms, took {elapsed:?}"
    );
    Ok(())
}

#[test]
fn test_agent_socket_heal_refuses_a_file_at_the_dir_path() -> Result<()> {
    // -- Setup & Fixtures: `--socket <some-file>/agent.sock`
    let sandbox = tempfile::tempdir()?;
    let parent = sandbox.path().join("data");
    std::fs::write(&parent, b"not a directory")?;
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o644))?;

    // -- Exec
    let refused = ensure_serving_dir(&parent);

    // -- Check
    let Err(SshAgentError::Socket(message)) = refused else {
        return Err(format!("a regular file is not a socket directory, got {refused:?}").into());
    };
    assert!(
        message.contains(&parent.display().to_string()),
        "the refusal names the path, got: {message}"
    );
    assert_eq!(
        mode_of(&parent)?,
        0o644,
        "a refusal must not chmod the file the operator pointed at"
    );
    assert_eq!(
        std::fs::read(&parent)?,
        b"not a directory",
        "the file content is untouched"
    );
    Ok(())
}

#[test]
fn test_agent_socket_heal_verdict_accepts_a_narrowed_restat() -> Result<()> {
    // -- Setup & Fixtures
    let sandbox = tempfile::tempdir()?;
    let dir = open_dir(sandbox.path())?;

    // -- Exec
    let healing = heal_verdict(&dir, 0o755, 0o700)?;

    // -- Check
    assert_eq!(healing, DirHeal::Narrowed { from: 0o755 });
    Ok(())
}

#[test]
fn test_agent_socket_heal_notice_names_the_dir_and_both_modes() {
    // -- Check
    assert_eq!(
        narrowing_notice(Path::new("/home/dev/.local/share/bogita-dev"), 0o755),
        "narrowing data dir /home/dev/.local/share/bogita-dev from 0o755 to 0o700",
        "the notice carries the path and the modes only"
    );
}

// endregion: --- Directory heal
