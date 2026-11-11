//! One agent connection: the shared ring, the peer-gate verdict, the in-flight
//! ticket the drain counts, and the daemon's own lock handler.

use crate::agent::control::{
    status_payload, Control, LOCK_EXTENSION, STATUS_EXTENSION, STOP_EXTENSION, UNLOCK_EXTENSION,
};
use crate::agent::error::SshAgentError;
use crate::agent::keyring::{rsa_hash_alg, SharedKeyRing};
use crate::agent::server::SessionTicket;
use crate::agent::unlock::{NoLock, VaultService};
use crate::domain::SecretString;
use ssh_agent_lib::agent::Session;
use ssh_agent_lib::error::AgentError;
use ssh_agent_lib::proto::{Extension, Identity, PublicCredential, SignRequest};
use ssh_key::Signature;
use std::sync::{Arc, MutexGuard};

/// One agent connection.
///
/// A session that does not pass the peer gate answers FAILURE to every request
/// and never touches the ring.
pub struct BogitaSession<L: VaultService = NoLock> {
    ring: SharedKeyRing,
    permits: bool,
    active: SessionTicket,
    lock: Arc<L>,
}

impl BogitaSession<NoLock> {
    /// A session that is not attached to a serve loop.
    pub fn new(ring: SharedKeyRing, permits: bool) -> Self {
        Self::tracked(ring, permits, Arc::new(Control::new()), Arc::new(NoLock))
    }
}

impl<L: VaultService + 'static> BogitaSession<L> {
    pub(super) fn tracked(
        ring: SharedKeyRing,
        permits: bool,
        control: Arc<Control>,
        lock: Arc<L>,
    ) -> Self {
        Self {
            ring,
            permits,
            active: SessionTicket::new(control),
            lock,
        }
    }

    fn ring(&self) -> MutexGuard<'_, crate::agent::keyring::KeyRing> {
        self.ring.lock().expect("key ring mutex poisoned")
    }
}

#[ssh_agent_lib::async_trait]
impl<L: VaultService + 'static> Session for BogitaSession<L> {
    async fn request_identities(&mut self) -> Result<Vec<Identity>, AgentError> {
        if !self.permits {
            return Err(AgentError::Failure);
        }

        let mut ring = self.ring();
        if ring.is_locked() {
            return Err(failure(SshAgentError::Locked));
        }

        let identities: Vec<Identity> = ring
            .entries()
            .iter()
            .map(|entry| Identity {
                credential: PublicCredential::Key(entry.public_key().clone()),
                comment: entry.comment().to_string(),
            })
            .collect();

        // A served request counts against the inactivity cap; a refusal does not.
        ring.note_request();
        Ok(identities)
    }

    async fn sign(&mut self, request: SignRequest) -> Result<Signature, AgentError> {
        if !self.permits {
            return Err(AgentError::Failure);
        }

        let mut ring = self.ring();
        let signature = ring
            .sign(
                request.credential.key_data(),
                &request.data,
                rsa_hash_alg(request.flags),
            )
            .map_err(failure)?;
        ring.note_request();
        Ok(signature)
    }

    async fn extension(&mut self, extension: Extension) -> Result<Option<Extension>, AgentError> {
        if !self.permits {
            return Err(AgentError::ExtensionFailure);
        }

        match extension.name.as_str() {
            STOP_EXTENSION => {
                self.active.0.request_stop();
                Ok(None)
            }
            STATUS_EXTENSION => {
                let ring = self.ring();
                let payload = status_payload(ring.len(), ring.is_locked());
                Ok(Some(Extension {
                    name: extension.name,
                    details: payload.into_bytes().into(),
                }))
            }
            // The passphrase is derived, used, and dropped here: a refusal answers
            // FAILURE with no payload, and nothing about the attempt is logged.
            UNLOCK_EXTENSION => {
                let Ok(passphrase) = std::str::from_utf8(extension.details.as_ref()) else {
                    return Err(AgentError::Failure);
                };
                let passphrase = SecretString::from(passphrase.to_string());
                match self.lock.unlock(&passphrase).await {
                    Ok(_keys) => Ok(None),
                    Err(_) => Err(AgentError::Failure),
                }
            }
            LOCK_EXTENSION => {
                self.lock.lock();
                Ok(None)
            }
            _ => Err(AgentError::ExtensionFailure),
        }
    }
}

/// Map a daemon error onto the generic agent failure (FAILURE(5) on the wire).
fn failure(error: SshAgentError) -> AgentError {
    AgentError::other(error)
}

// region:    --- Tests
#[cfg(test)]
#[path = "../../../test/agent/session.rs"]
mod session_test;
// endregion: --- Tests
