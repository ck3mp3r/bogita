//! Relay of the git signing helper onto the agent daemon.
//!
//! git's `-Y sign` and `gpg.ssh.defaultKeyCommand` are answered from the daemon's
//! ring, never from vault entries: the helper reads the requested public key,
//! asks the daemon which keys it serves, relays one SIGN_REQUEST, and writes the
//! SshSig armor git expects. No private key material enters this process, so the
//! helper needs no passphrase and never prompts.
//!
//! The daemon operations sit behind [`AgentRelay`]: the relay logic is tested
//! against a mock, and production passes [`DaemonRelay`], which owns the socket.

use crate::handlers::ssh_sign::{
    read, read_public_key, signature_path, write, HelperError, SignRequest, SIGN_HASH_ALG,
};
use bogita_core::agent::client;
use bogita_core::agent::error::SshAgentError;
use ssh_key::{Algorithm, LineEnding, PublicKey, Signature, SshSig};
use std::path::PathBuf;

// region:    --- Constants

/// Bit a SIGN_REQUEST sets to ask for an rsa-sha2-512 reply (RFC 9987 § 8.3).
///
/// `rsa_hash_alg` in `crates/core/src/agent/keyring.rs` reads it: `0x4` answers
/// rsa-sha2-512, anything else answers rsa-sha2-256. Ed25519 sends no flag.
pub const RSA_SHA2_512_FLAG: u32 = 0x0000_0004;

// endregion: --- Constants

// region:    --- Types

/// Why one daemon exchange failed.
#[derive(Debug)]
pub enum RelayError {
    /// The daemon answered FAILURE(5): its ring does not hold the key.
    Refused,
    /// The exchange failed: nothing answers, or the reply is not a signature.
    Unavailable,
}

/// The daemon operations the git helper relays onto.
///
/// Signing and identity listing are the only two the helper needs; both answer
/// from the daemon's in-memory ring.
pub trait AgentRelay {
    /// Public keys the daemon serves, with their comments.
    fn identities(&mut self) -> Result<Vec<PublicKey>, RelayError>;

    /// Sign `data` with `key` under the SIGN_REQUEST `flags` bitfield.
    fn sign(&mut self, key: &PublicKey, data: &[u8], flags: u32) -> Result<Signature, RelayError>;
}

/// The daemon listening on one unix socket: the production [`AgentRelay`].
pub struct DaemonRelay {
    socket: PathBuf,
}

impl DaemonRelay {
    /// Relay onto the daemon that listens on `socket`.
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
        }
    }
}

impl AgentRelay for DaemonRelay {
    fn identities(&mut self) -> Result<Vec<PublicKey>, RelayError> {
        client::identities(&self.socket).map_err(|_| RelayError::Unavailable)
    }

    fn sign(&mut self, key: &PublicKey, data: &[u8], flags: u32) -> Result<Signature, RelayError> {
        match client::sign(&self.socket, key.key_data(), data, flags) {
            Ok(signature) => Ok(signature),
            Err(SshAgentError::UnknownKey) => Err(RelayError::Refused),
            Err(_) => Err(RelayError::Unavailable),
        }
    }
}

// endregion: --- Types

// region:    --- Relay

/// Relay `-Y sign`: sign the SSHSIG payload with the daemon's matching key, and
/// write the armor to `<data-file>.sig`.
///
/// The requested public key decides the signer: the daemon signs with the cached
/// key whose wire form matches, so a differing comment does not matter. A key the
/// ring does not hold answers [`HelperError::NoMatchingKey`], and a daemon that
/// stops answering answers [`HelperError::Locked`] - the same fail-secure answer
/// the status gate gives.
pub fn sign(relay: &mut impl AgentRelay, request: &SignRequest) -> Result<(), HelperError> {
    let requested = read_public_key(&request.key_file)?;
    let served = relay.identities().map_err(|_| HelperError::Locked)?;
    let key = served
        .iter()
        .find(|key| key.key_data() == requested.key_data())
        .ok_or(HelperError::NoMatchingKey)?;

    let data = read(&request.data_file)?;
    let payload = SshSig::signed_data(&request.namespace, SIGN_HASH_ALG, &data)
        .map_err(|error| HelperError::Key(error.to_string()))?;
    let signature = relay
        .sign(key, &payload, flags_for(key))
        .map_err(|error| match error {
            RelayError::Refused => HelperError::NoMatchingKey,
            RelayError::Unavailable => HelperError::Locked,
        })?;

    let armor = SshSig::new(
        key.key_data().clone(),
        &request.namespace,
        SIGN_HASH_ALG,
        signature,
    )
    .and_then(|signature| signature.to_pem(LineEnding::LF))
    .map_err(|error| HelperError::Key(error.to_string()))?;
    write(&signature_path(&request.data_file), armor.as_bytes())
}

/// Relay `ssh-default-key`: the public key line of the only key the daemon serves.
///
/// Zero or several served keys are an error: the helper never picks silently.
pub fn default_key(relay: &mut impl AgentRelay) -> Result<String, HelperError> {
    let served = relay.identities().map_err(|_| HelperError::Locked)?;
    match served.as_slice() {
        [] => Err(HelperError::NoAgentServableKeys),
        [key] => key
            .to_openssh()
            .map_err(|error| HelperError::Key(error.to_string())),
        _ => Err(HelperError::MultipleAgentServableKeys),
    }
}

/// SIGN_REQUEST flags for `key`.
///
/// RSA signatures name a digest, and SSH's own default is rsa-sha2-512 for git
/// signatures, so the helper asks for it. Ed25519 has no digest choice.
fn flags_for(key: &PublicKey) -> u32 {
    match key.algorithm() {
        Algorithm::Rsa { .. } => RSA_SHA2_512_FLAG,
        _ => 0,
    }
}

// endregion: --- Relay

// region:    --- Tests
#[cfg(test)]
#[path = "../../test/handlers/ssh_relay.rs"]
mod ssh_relay_test;

#[cfg(all(test, feature = "integration"))]
#[path = "../../test/handlers/ssh_sign_relay.rs"]
mod ssh_sign_relay_test;
// endregion: --- Tests
