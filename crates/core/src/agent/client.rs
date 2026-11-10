//! Blocking client for the agent control channel and for identity listing.
//!
//! The CLI speaks to the daemon over the same unix socket that `ssh` uses. The
//! client is blocking on purpose: every command is one request/response exchange
//! run once by another short-lived process.

use crate::agent::control::{STATUS_EXTENSION, STOP_EXTENSION};
use crate::agent::error::SshAgentError;
use ssh_agent_lib::blocking::Client;
use ssh_agent_lib::proto::Extension;
use ssh_key::{HashAlg, PublicKey};
use std::io::ErrorKind;
use std::os::unix::net::UnixStream;
use std::path::Path;

// region:    --- Types

/// One key served by the daemon, as advertised by IDENTITIES_ANSWER(12).
pub struct ServedKey {
    /// SHA-256 fingerprint as `ssh_key::Fingerprint` displays it: `SHA256:<base64>`.
    /// The bit length, comment, and algorithm suffix that `ssh-keygen -l` adds are absent.
    pub fingerprint: String,
    /// Comment advertised with the public key.
    pub comment: String,
}

// endregion: --- Types

// region:    --- Commands

/// True when a process answers on `path`.
pub fn responds(path: &Path) -> bool {
    UnixStream::connect(path).is_ok()
}

/// Ask the daemon on `path` to stop.
///
/// Returns once the daemon answered SUCCESS(6); the daemon finishes its drain
/// and unlinks the socket afterwards.
pub fn stop(path: &Path) -> Result<(), SshAgentError> {
    let mut client = running_client(path)?;
    match client
        .extension(request(STOP_EXTENSION))
        .map_err(control_error)?
    {
        None => Ok(()),
        Some(reply) => Err(SshAgentError::Control(format!(
            "unexpected '{}' reply to a stop request",
            reply.name
        ))),
    }
}

/// Status payload of the daemon on `path`, or `None` when nothing answers.
pub fn status(path: &Path) -> Result<Option<String>, SshAgentError> {
    let Some(mut client) = connect(path)? else {
        return Ok(None);
    };
    match client
        .extension(request(STATUS_EXTENSION))
        .map_err(control_error)?
    {
        Some(reply) => payload_text(&reply).map(Some),
        None => Err(SshAgentError::Control(
            "status answered SUCCESS without a payload".to_string(),
        )),
    }
}

/// Keys served by the daemon on `path`.
pub fn keys(path: &Path) -> Result<Vec<ServedKey>, SshAgentError> {
    let mut client = running_client(path)?;
    let identities = client.request_identities().map_err(control_error)?;
    Ok(identities
        .into_iter()
        .map(|identity| ServedKey {
            fingerprint: PublicKey::from(identity.credential.key_data().clone())
                .fingerprint(HashAlg::Sha256)
                .to_string(),
            comment: identity.comment,
        })
        .collect())
}

// endregion: --- Commands

// region:    --- Support

/// A control request: an extension name without a payload.
fn request(name: &str) -> Extension {
    Extension {
        name: name.to_string(),
        details: Vec::new().into(),
    }
}

/// Connect to the daemon, or report that nothing answers on `path`.
fn connect(path: &Path) -> Result<Option<Client<UnixStream>>, SshAgentError> {
    match UnixStream::connect(path) {
        Ok(stream) => Ok(Some(Client::new(stream))),
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused) => {
            Ok(None)
        }
        Err(e) => Err(SshAgentError::NotRunning(format!(
            "{}: {e}",
            path.display()
        ))),
    }
}

/// Connect to the daemon, requiring that it answers.
fn running_client(path: &Path) -> Result<Client<UnixStream>, SshAgentError> {
    connect(path)?.ok_or_else(|| SshAgentError::NotRunning(path.display().to_string()))
}

/// Decode the status payload of an EXTENSION_RESPONSE.
fn payload_text(reply: &Extension) -> Result<String, SshAgentError> {
    std::str::from_utf8(reply.details.as_ref())
        .map(str::to_string)
        .map_err(|e| SshAgentError::Control(format!("status payload is not UTF-8: {e}")))
}

/// Map a protocol error onto the control-channel error.
fn control_error(error: ssh_agent_lib::error::AgentError) -> SshAgentError {
    SshAgentError::Control(error.to_string())
}

// endregion: --- Support
