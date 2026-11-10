//! Agent socket path derivation, permission hardening, and the peer-uid gate.
//!
//! Path rule (G2): `$XDG_DATA_HOME/bogita[-dev]/agent.sock`, derived from the
//! same `APP_NAME` gate as the rest of the data dir. The directory is 0700 and
//! the socket file 0600, so filesystem permissions are the primary control.
//! After accept, the peer uid is compared with the daemon's uid; a mismatch
//! refuses the connection. If the peer check is unavailable on the platform the
//! connection is refused too — the gate fails closed.

use crate::agent::error::SshAgentError;
use std::io::Write;
use std::os::fd::RawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

// region:    --- Constants

/// Socket file name inside the application data dir.
pub const SOCKET_FILE_NAME: &str = "agent.sock";

/// Owner-only directory mode.
pub const DIR_MODE: u32 = 0o700;

/// Owner-only socket mode.
pub const SOCKET_MODE: u32 = 0o600;

/// Mode bits that let another user reach the socket directory.
const SHARED_BITS: u32 = 0o077;

// endregion: --- Constants

// region:    --- Path

/// `$XDG_DATA_HOME/bogita[-dev]/agent.sock`.
pub fn default_socket_path() -> PathBuf {
    crate::storage::config::default_data_dir().join(SOCKET_FILE_NAME)
}

// endregion: --- Path

// region:    --- Preparation

/// What the parent-side pre-check did to the socket directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirHeal {
    /// The directory was missing and is created 0700.
    Created,
    /// The directory carries no group or other bit and is left alone.
    AlreadyPrivate,
    /// The directory carried open bits and is narrowed from `from`.
    Narrowed { from: u32 },
}

/// Create the socket directory 0700, or heal a pre-existing mode that another user
/// can reach.
///
/// `agent start` runs this before it spawns the daemon, so the bind gate
/// ([`prepare`]) meets a private directory on the normal path — including the data
/// dir an older bogita created 0755. The healing is verified with a second `stat`
/// ([`heal_verdict`]), because a `chmod` cannot prove that the narrowing took hold.
/// Only a directory whose mode stays open is refused, with the bind gate's message.
/// A path that is not a directory is refused before any mode change, so an operator
/// mistake such as `--socket /some/file/agent.sock` cannot chmod the file it names.
/// The daemon takes care of its own directory state; no manual `chmod` is required.
pub fn ensure_serving_dir(dir: &Path) -> Result<DirHeal, SshAgentError> {
    match std::fs::metadata(dir) {
        Ok(metadata) if !metadata.is_dir() => Err(SshAgentError::Socket(format!(
            "{} is not a directory; the agent serves only from a directory",
            dir.display()
        ))),
        Ok(metadata) => heal_dir_mode(dir, metadata.permissions().mode()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            create_private_dir_new(dir)?;
            Ok(DirHeal::Created)
        }
        Err(e) => Err(SshAgentError::Socket(e.to_string())),
    }
}

/// Narrow an existing directory that others can reach, then verify the result.
fn heal_dir_mode(dir: &Path, mode: u32) -> Result<DirHeal, SshAgentError> {
    let before = mode & 0o777;
    if before & SHARED_BITS == 0 {
        return Ok(DirHeal::AlreadyPrivate);
    }
    report(&narrowing_notice(dir, before));
    set_mode(dir, DIR_MODE)?;
    let after = std::fs::metadata(dir).map_err(|e| SshAgentError::Socket(e.to_string()))?;
    heal_verdict(dir, before, after.permissions().mode())
}

/// Decide what a fresh `stat` taken after the narrowing attempt proves.
///
/// The verdict comes from `mode_after`, never from the mode the `chmod` asked for: an
/// open handle or a concurrent change can leave a directory wide after a successful
/// call, and serving from it would be the narrow-then-serve fail-open defect. A mode
/// that is still open is refused through [`require_private_dir`], so the message is the
/// bind gate's, word for word, and the caller exits instead of waiting for the spawn
/// timeout.
fn heal_verdict(dir: &Path, mode_before: u32, mode_after: u32) -> Result<DirHeal, SshAgentError> {
    if mode_after & SHARED_BITS != 0 {
        // The gate refuses any mode that is not owner-only, so `?` returns its message.
        require_private_dir(dir, mode_after)?;
    }
    Ok(DirHeal::Narrowed {
        from: mode_before & 0o777,
    })
}

/// The one-line notice a healing start prints: path and modes only.
fn narrowing_notice(dir: &Path, from: u32) -> String {
    format!(
        "narrowing data dir {} from 0o{:o} to 0o{DIR_MODE:o}",
        dir.display(),
        from & 0o777
    )
}

/// Write one lifecycle line to descriptor 2.
///
/// The write goes through the descriptor rather than `eprintln!`, so the line is the
/// same for the user and stays observable to a test harness that sinks print-macro
/// output.
fn report(line: &str) {
    let _ = writeln!(std::io::stderr(), "{line}");
}

/// Create the socket directory (0700) and clear a stale socket file.
///
/// Returns [`SshAgentError::AlreadyServing`] when a process answers on
/// `path`. A socket file that nothing answers on is removed, so a crashed
/// daemon does not block the next start.
///
/// A directory that already exists must carry no group or other bit. The parent heals
/// the normal path before it spawns the daemon ([`ensure_serving_dir`]); this gate stays
/// as the race net, because narrowing from here would not be fail-closed: any process
/// that opened the directory before the change keeps its access to it. The daemon
/// therefore refuses and names the mode, and a bogita that creates the directory itself
/// creates it 0700 from the first instant
/// ([`crate::storage::config::ensure_private_dir`]).
pub fn prepare(path: &Path) -> Result<(), SshAgentError> {
    let dir = path
        .parent()
        .ok_or_else(|| SshAgentError::Socket(format!("{} has no parent", path.display())))?;
    create_private_dir(dir)?;

    if path.exists() {
        match std::os::unix::net::UnixStream::connect(path) {
            Ok(_) => {
                return Err(SshAgentError::AlreadyServing(path.display().to_string()));
            }
            Err(_) => remove_socket_file(path)?,
        }
    }
    Ok(())
}

/// Restrict the bound socket file to 0600.
pub fn harden_socket(path: &Path) -> Result<(), SshAgentError> {
    set_mode(path, SOCKET_MODE)
}

fn create_private_dir(dir: &Path) -> Result<(), SshAgentError> {
    match std::fs::metadata(dir) {
        Ok(metadata) => require_private_dir(dir, metadata.permissions().mode()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => create_private_dir_new(dir),
        Err(e) => Err(SshAgentError::Socket(e.to_string())),
    }
}

/// Create a directory tree that nobody but the owner can reach.
fn create_private_dir_new(dir: &Path) -> Result<(), SshAgentError> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(DIR_MODE)
        .create(dir)
        .map_err(|e| SshAgentError::Socket(e.to_string()))
}

/// Refuse a pre-existing socket directory that carries any mode but 0700.
fn require_private_dir(dir: &Path, mode: u32) -> Result<(), SshAgentError> {
    let mode = mode & 0o777;
    if mode == DIR_MODE {
        return Ok(());
    }
    Err(SshAgentError::Socket(format!(
        "{} has mode 0o{mode:o}; the agent serves only from a 0o{DIR_MODE:o} directory (chmod 700 '{}')",
        dir.display(),
        dir.display()
    )))
}

fn set_mode(path: &Path, mode: u32) -> Result<(), SshAgentError> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|e| SshAgentError::Socket(e.to_string()))
}

fn remove_socket_file(path: &Path) -> Result<(), SshAgentError> {
    std::fs::remove_file(path).map_err(|e| SshAgentError::Socket(e.to_string()))
}

// endregion: --- Preparation

// region:    --- Umask

/// Restricts the process umask to owner-only while it is alive.
///
/// `UnixListener::bind` creates the socket file with the process umask applied, and
/// [`harden_socket`] narrows it to 0600 only afterwards, so on a virgin data dir the file
/// would carry the umask mode for a moment. The umask closes that window. The socket
/// directory is not affected: [`prepare`] creates it 0700 in one step. The peer-uid gate
/// refuses foreign connections throughout. The previous mask is restored on drop, so later
/// files keep their configured permissions.
#[derive(Debug)]
pub struct PrivateUmask {
    previous: libc::mode_t,
}

impl PrivateUmask {
    /// Set the process umask to 0077 and remember the previous value.
    pub fn set() -> Self {
        // SAFETY: `umask` takes one argument, touches no memory, and may be
        // called from any thread. It is process-wide state; the guard restores
        // the previous value when it drops.
        let previous = unsafe { libc::umask(0o077) };
        Self { previous }
    }
}

impl Drop for PrivateUmask {
    fn drop(&mut self) {
        // SAFETY: same contract as `PrivateUmask::set`.
        unsafe { libc::umask(self.previous) };
    }
}

// endregion: --- Umask

// region:    --- Peer gate

/// True when the connected peer belongs to the expected user.
pub fn peer_matches(peer_uid: u32, expected_uid: u32) -> bool {
    peer_uid == expected_uid
}

/// uid of the process on the other end of `stream`.
pub fn peer_uid(stream: &tokio::net::UnixStream) -> Result<u32, SshAgentError> {
    use std::os::fd::AsRawFd;
    peer_uid_from_fd(stream.as_raw_fd())
}

/// uid of the running process, which is the only uid the agent serves.
pub fn current_uid() -> u32 {
    // SAFETY: `geteuid` takes no arguments, cannot fail, and touches no memory.
    unsafe { libc::geteuid() }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn peer_uid_from_fd(fd: RawFd) -> Result<u32, SshAgentError> {
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;

    // SAFETY: `fd` is an open socket, `credentials`/`length` describe a valid
    // writable buffer, and the kernel writes at most `length` bytes into it.
    let status = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut credentials as *mut libc::ucred as *mut libc::c_void,
            &mut length,
        )
    };
    if status != 0 {
        return Err(SshAgentError::Socket(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    Ok(credentials.uid)
}

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
fn peer_uid_from_fd(fd: RawFd) -> Result<u32, SshAgentError> {
    let mut uid: libc::uid_t = 0;
    let mut gid: libc::gid_t = 0;

    // SAFETY: `fd` is an open socket and both out-parameters point at valid
    // writable `uid_t`/`gid_t` values.
    let status = unsafe { libc::getpeereid(fd, &mut uid, &mut gid) };
    if status != 0 {
        return Err(SshAgentError::Socket(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    Ok(uid)
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
)))]
fn peer_uid_from_fd(_fd: RawFd) -> Result<u32, SshAgentError> {
    Err(SshAgentError::Socket(
        "peer credentials are not available on this platform".to_string(),
    ))
}

// endregion: --- Peer gate

// region:    --- Guard

/// Device and inode that identify one socket file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

/// Removes the socket file this daemon bound — and only that file.
///
/// A daemon that is draining can outlive its own socket: the stop path unlinks
/// before the drain, so a restart may bind a new file at the same path. The
/// guard therefore removes the path only while it still carries the device and
/// inode captured at bind time. An absent path, or one a restart has replaced,
/// is left alone.
#[derive(Debug)]
pub struct SocketGuard {
    path: PathBuf,
    identity: FileIdentity,
}

impl SocketGuard {
    /// Capture the identity of the socket file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, SshAgentError> {
        let path = path.into();
        let identity = file_identity(&path)?;
        Ok(Self { path, identity })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Unlink the socket file if it is still the file this guard captured.
    pub fn remove(&self) {
        if self.owns_current_file() {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn owns_current_file(&self) -> bool {
        file_identity(&self.path).is_ok_and(|identity| identity == self.identity)
    }
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        self.remove();
    }
}

/// Device and inode of the file at `path`.
fn file_identity(path: &Path) -> Result<FileIdentity, SshAgentError> {
    let metadata = std::fs::metadata(path).map_err(|e| SshAgentError::Socket(e.to_string()))?;
    Ok(FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

// endregion: --- Guard

// region:    --- Tests
#[cfg(test)]
#[path = "../../test/agent/socket.rs"]
mod socket_test;
// endregion: --- Tests
