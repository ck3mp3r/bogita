//! Errors raised by the SSH agent daemon.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SshAgentError {
    /// The socket path cannot be derived, created, or restricted.
    #[error("agent socket error: {0}")]
    Socket(String),

    /// Another process answers on the socket path.
    #[error("another agent is already listening on {0}")]
    AlreadyServing(String),

    /// The serve loop stopped with an I/O error.
    #[error("agent serve loop failed: {0}")]
    Serve(String),

    /// The vault is locked, so the ring holds no keys.
    #[error("vault is locked")]
    Locked,

    /// The request names a key the ring does not hold.
    #[error("unknown key")]
    UnknownKey,

    /// The key backend refused to produce a signature.
    #[error("signing failed: {0}")]
    SignFailed(String),

    /// No process answers on the agent socket.
    #[error("no agent is running on {0}")]
    NotRunning(String),

    /// The control extension failed or answered an unexpected message.
    #[error("agent control channel failed: {0}")]
    Control(String),

    /// The daemon child could not be spawned or never opened its socket.
    #[error("failed to start the agent daemon: {0}")]
    Spawn(String),
}
