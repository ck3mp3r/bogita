//! Blocking client for the agent control channel and for identity listing.
//!
//! The CLI speaks to the daemon over the same unix socket that `ssh` uses. The
//! client is blocking on purpose: every command is one request/response exchange
//! run once by another short-lived process.

use crate::agent::control::{LOCK_EXTENSION, STATUS_EXTENSION, STOP_EXTENSION, UNLOCK_EXTENSION};
use crate::agent::error::SshAgentError;
use secrecy::{ExposeSecret, SecretString};
use ssh_agent_lib::blocking::Client;
use ssh_agent_lib::error::AgentError;
use ssh_agent_lib::proto::{Extension, ProtoError, PublicCredential, SignRequest};
use ssh_key::public::KeyData;
use ssh_key::{HashAlg, PublicKey, Signature};
use std::io::ErrorKind;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

// region:    --- Constants

/// Upper bound for one control exchange.
///
/// A daemon that accepts the connection and never answers would otherwise park
/// the caller forever; the TUI's exit lock runs on this path. Five seconds is far
/// above a live daemon's answer and short enough that an exit still completes.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

// endregion: --- Constants

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

/// Ask the daemon on `path` to unlock with `passphrase`.
///
/// Answers SUCCESS(6) once the ring serves the vault. A refused passphrase maps
/// onto [`SshAgentError::UnlockRefused`]; the payload is never echoed.
pub fn unlock(path: &Path, passphrase: &SecretString) -> Result<(), SshAgentError> {
    let mut client = running_client(path)?;
    let extension = Extension {
        name: UNLOCK_EXTENSION.to_string(),
        details: passphrase.expose_secret().as_bytes().to_vec().into(),
    };
    match client.extension(extension) {
        Ok(None) => Ok(()),
        Ok(Some(reply)) => Err(SshAgentError::Control(format!(
            "unexpected '{}' reply to an unlock request",
            reply.name
        ))),
        // The blocking client folds every non-SUCCESS reply into
        // `ProtoError::UnexpectedResponse`, so a refused passphrase arrives as that
        // error and not as `AgentError::Failure`.
        Err(AgentError::Failure) | Err(AgentError::Proto(ProtoError::UnexpectedResponse)) => {
            Err(SshAgentError::UnlockRefused)
        }
        Err(e) => Err(control_error(e)),
    }
}

/// Ask the daemon on `path` to lock: it evicts the ring.
pub fn lock(path: &Path) -> Result<(), SshAgentError> {
    let mut client = running_client(path)?;
    match client
        .extension(request(LOCK_EXTENSION))
        .map_err(control_error)?
    {
        None => Ok(()),
        Some(reply) => Err(SshAgentError::Control(format!(
            "unexpected '{}' reply to a lock request",
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

/// Public keys the daemon on `path` advertises, with their comments.
///
/// This is IDENTITIES_ANSWER(12) as the caller needs it: the wire form of every
/// served key. [`keys`] answers the display shape of the same reply.
pub fn identities(path: &Path) -> Result<Vec<PublicKey>, SshAgentError> {
    let mut client = running_client(path)?;
    let identities = client.request_identities().map_err(control_error)?;
    Ok(identities
        .into_iter()
        .map(|identity| PublicKey::new(identity.credential.key_data().clone(), identity.comment))
        .collect())
}

/// Sign `data` with the daemon's cached key `public_key`.
///
/// `flags` is the SIGN_REQUEST bitfield of RFC 9987 § 8.3; the daemon maps it
/// onto the digest an RSA reply names, and an Ed25519 reply ignores it. The
/// signature is the daemon's, so no key material reaches this process.
///
/// A FAILURE(5) reply is [`SshAgentError::UnknownKey`]: the caller checks the
/// daemon's lock state first, so a refusal here means the ring does not hold the
/// key.
pub fn sign(
    path: &Path,
    public_key: &KeyData,
    data: &[u8],
    flags: u32,
) -> Result<Signature, SshAgentError> {
    let mut client = running_client(path)?;
    let request = SignRequest {
        credential: PublicCredential::Key(public_key.clone()),
        data: data.to_vec(),
        flags,
    };
    match client.sign(request) {
        Ok(signature) => Ok(signature),
        // The blocking client folds every non-SIGN_RESPONSE reply into
        // `ProtoError::UnexpectedResponse`, so FAILURE(5) arrives as that error.
        Err(AgentError::Failure) | Err(AgentError::Proto(ProtoError::UnexpectedResponse)) => {
            Err(SshAgentError::UnknownKey)
        }
        Err(e) => Err(control_error(e)),
    }
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
///
/// Every exchange on the returned client is bounded by [`READ_TIMEOUT`]: a
/// daemon that accepts and never answers cannot park the caller.
fn connect(path: &Path) -> Result<Option<Client<UnixStream>>, SshAgentError> {
    match UnixStream::connect(path) {
        Ok(stream) => {
            stream
                .set_read_timeout(Some(READ_TIMEOUT))
                .map_err(|e| SshAgentError::Control(format!("{}: {e}", path.display())))?;
            Ok(Some(Client::new(stream)))
        }
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
fn control_error(error: AgentError) -> SshAgentError {
    SshAgentError::Control(error.to_string())
}

// endregion: --- Support

// region:    --- Tests
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/agent/client.rs"]
mod client_test;
// endregion: --- Tests
