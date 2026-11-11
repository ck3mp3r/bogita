//! In-memory SSH key cache for the agent ("unlock-time cache").
//!
//! The ring parses every servable key once, when the vault is unlocked, and the
//! serve loop reuses the cached `ssh_key::PrivateKey` for each SIGN_REQUEST.
//! Decrypting and parsing per request was rejected: it adds a database decrypt
//! and an OpenSSH parse to every signature. Holding keys in RAM while unlocked
//! is the normal exposure window of an SSH agent (1Password and Proton Pass do
//! the same); the cache is evicted by the vault service's own `lock`, which is
//! also what `IdleWatch` applies when the idle cap elapses.
//!
//! Nothing here logs key material: a key that fails to parse is reported by
//! entry name only.

use crate::agent::error::SshAgentError;
use crate::domain::ssh::{key_type_name, AGENT_SERVABLE_KEY};
use crate::domain::{AgeIdentity, Entry, FieldType, FieldValue};
use crate::error::Result;
use crate::ports::{EntryStore, VaultStore};
use signature::{SignatureEncoding, Signer};
use ssh_key::private::KeypairData;
use ssh_key::public::KeyData;
use ssh_key::sha2::{Sha256, Sha512};
use ssh_key::{Algorithm, HashAlg, Mpint, PrivateKey, Signature};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Shared handle to the ring. Sessions clone the `Arc`, never the keys.
pub type SharedKeyRing = Arc<Mutex<KeyRing>>;

// region:    --- Entry

/// One servable key: the parsed private key plus its public wire form.
///
/// `ssh-key` zeroizes key scalars when the value drops (Ed25519 seed, RSA
/// primes, DSA exponent), so dropping the cache is the eviction step.
///
/// Deliberately not `Clone`: duplicating a cache entry would duplicate key
/// material outside the ring's control.
pub struct KeyRingEntry {
    entry_id: Uuid,
    name: String,
    comment: String,
    public_key: KeyData,
    private_key: PrivateKey,
}

impl KeyRingEntry {
    /// Id of the vault entry this key came from.
    pub fn entry_id(&self) -> Uuid {
        self.entry_id
    }

    /// Name of the vault entry this key came from. Safe to log.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Comment advertised to clients (the `SshComment` field value).
    pub fn comment(&self) -> &str {
        &self.comment
    }

    /// Public key in wire form, as advertised by IDENTITIES_ANSWER.
    pub fn public_key(&self) -> &KeyData {
        &self.public_key
    }
}

impl std::fmt::Debug for KeyRingEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyRingEntry")
            .field("entry_id", &self.entry_id)
            .field("name", &self.name)
            .field("comment", &self.comment)
            .finish_non_exhaustive()
    }
}

// endregion: --- Entry

// region:    --- Ring

/// Unlock-time cache of servable keys.
///
/// `None` means "locked" — every request answers FAILURE. `Some(entries)` means
/// the vault is unlocked; the vector may be empty.
///
/// Two lock-policy fields travel with the cache:
///
/// - `last_served_request` is the activity clock of the inactivity cap. Only a
///   successful request stamps it; refusals do not, so a client that keeps asking
///   against a locked ring cannot keep the ring alive. `IdleWatch::step` is the
///   comparator.
/// - `rebuild_allowed` is the second line of defence for the residency cap. Lock
///   clears it and only `VaultLock::unlock` sets it, so a rebuild path that some
///   later caller adds cannot revive an evicted ring on the strength of a request.
#[derive(Default)]
pub struct KeyRing {
    entries: Option<Vec<KeyRingEntry>>,
    last_served_request: Option<Instant>,
    rebuild_allowed: bool,
}

impl KeyRing {
    /// A locked ring: no key material, every request refused.
    pub fn new() -> Self {
        Self::default()
    }

    /// True while the ring holds no key material.
    pub fn is_locked(&self) -> bool {
        self.entries.is_none()
    }

    /// Number of cached keys.
    pub fn len(&self) -> usize {
        self.entries().len()
    }

    /// True when no key is cached (locked or unlocked with zero servable keys).
    pub fn is_empty(&self) -> bool {
        self.entries().is_empty()
    }

    /// Cached keys in insertion order; empty while locked.
    pub fn entries(&self) -> &[KeyRingEntry] {
        self.entries.as_deref().unwrap_or(&[])
    }

    /// Drop the cache. The next request answers FAILURE until the ring is
    /// populated again.
    pub fn clear(&mut self) {
        self.entries = None;
        self.last_served_request = None;
    }

    /// Instant of the last request this ring served successfully.
    ///
    /// `IdleWatch::step` compares it against the vault's `lock_timeout`; `None`
    /// means the ring holds no cache, so there is nothing to evict.
    pub fn last_served_request(&self) -> Option<Instant> {
        self.last_served_request
    }

    /// Count one served request against the inactivity cap.
    pub fn note_request(&mut self) {
        self.last_served_request = Some(Instant::now());
    }

    /// True while a rebuild of the cache is permitted.
    pub fn rebuild_allowed(&self) -> bool {
        self.rebuild_allowed
    }

    /// Permit one rebuild: `VaultLock::unlock` started an unlock.
    pub fn allow_rebuild(&mut self) {
        self.rebuild_allowed = true;
    }

    /// Refuse rebuilds until `VaultLock::unlock` starts another unlock.
    pub fn forbid_rebuild(&mut self) {
        self.rebuild_allowed = false;
    }

    /// Install a freshly loaded cache, unless the lock policy forbids it.
    ///
    /// Returns `false` when the rebuild was refused, which leaves the ring exactly
    /// as it was: locked and evicted.
    ///
    /// A permitted rebuild consumes the permit, so `rebuild_allowed` is false once
    /// the cache is loaded: the permit exists only across an in-flight unlock, and a
    /// refused or failed one leaves the ring forbidden.
    pub fn rebuild(&mut self, entries: impl IntoIterator<Item = Entry>) -> bool {
        if !self.rebuild_allowed {
            return false;
        }
        self.load_entries(entries);
        self.rebuild_allowed = false;
        true
    }

    /// Replace the cache with the servable keys found in `entries`.
    ///
    /// Returns the number of keys cached. Two skip classes are silent: an entry
    /// without the plaintext `agent_servable = true` gate, and an entry without an
    /// `SshPrivateKey` field. Three skip classes are reported by entry name and
    /// reason: a passphrase-protected key, a key outside the support matrix, and a
    /// key that does not parse. No report carries key material, so nothing reaches
    /// the log.
    ///
    /// Only keys [`algorithm_supported`] accepts are advertised: `ssh-key` is built
    /// with the `ed25519` and `rsa` features, so a key of any other type has no
    /// signing arm. Advertising such a key would make IDENTITIES_ANSWER promise a
    /// signature that SIGN_REQUEST cannot produce, so the filter sits here, at
    /// load, and not at sign. Keys imported from outside (`bogita entry keygen`
    /// emits ed25519) are therefore named in a warning and not served.
    ///
    /// Legacy rows may carry field_type custom:agent_servable; the gate reads that
    /// shape, and the next save of such a row rewrites it to boolean.
    pub fn load_entries(&mut self, entries: impl IntoIterator<Item = Entry>) -> usize {
        let mut cache = Vec::new();
        for entry in entries {
            if !is_agent_servable(&entry) {
                continue;
            }
            let Some(pem) = private_pem(&entry) else {
                continue;
            };
            let pem = Zeroizing::new(pem);
            let key = match PrivateKey::from_openssh(pem.as_bytes()) {
                Ok(key) if key.is_encrypted() => {
                    skip(&entry, "the key is passphrase protected");
                    continue;
                }
                Ok(key) if !algorithm_supported(key.algorithm()) => {
                    skip(&entry, &Self::unsupported_algorithm_reason(key.algorithm()));
                    continue;
                }
                Ok(key) => key,
                Err(_) => {
                    skip(&entry, "the key does not parse");
                    continue;
                }
            };
            let comment = ssh_comment(&entry).unwrap_or_else(|| key.comment().to_string());
            cache.push(KeyRingEntry {
                entry_id: entry.id,
                name: entry.name,
                comment,
                public_key: key.public_key().key_data().clone(),
                private_key: key,
            });
        }

        let count = cache.len();
        self.entries = Some(cache);
        self.last_served_request = Some(Instant::now());
        count
    }

    /// List the servable keys of every vault and cache them.
    ///
    /// Uses the same ports as the TUI (`list_vaults` + `list_entries` through
    /// the caller's identity), so no SQL is duplicated here. Returns the number
    /// of keys cached.
    pub async fn populate<S>(&mut self, storage: &S, identity: &AgeIdentity) -> Result<usize>
    where
        S: VaultStore + EntryStore,
    {
        let mut entries = Vec::new();
        for vault in storage.list_vaults().await? {
            entries.extend(storage.list_entries(vault.id, None, identity).await?);
        }
        Ok(self.load_entries(entries))
    }

    /// Sign `data` with the cached key whose public key is `public_key`.
    ///
    /// `hash` selects the digest an RSA reply advertises: the serve loop maps the
    /// SIGN_REQUEST flags onto it with [`rsa_hash_alg`]. Ed25519 ignores the
    /// parameter, because its reply algorithm follows the cached key.
    ///
    /// The dispatch is per algorithm and explicit: the cached key signs through
    /// the arm that matches its type, never through a default that would name a
    /// digest the client did not ask for.
    ///
    /// Returns [`SshAgentError::Locked`] when the ring is locked and
    /// [`SshAgentError::UnknownKey`] when the key is not cached.
    pub fn sign(
        &self,
        public_key: &KeyData,
        data: &[u8],
        hash: Option<HashAlg>,
    ) -> std::result::Result<Signature, SshAgentError> {
        let entries = self.entries.as_ref().ok_or(SshAgentError::Locked)?;
        let entry = entries
            .iter()
            .find(|entry| &entry.public_key == public_key)
            .ok_or(SshAgentError::UnknownKey)?;

        match entry.private_key.key_data() {
            KeypairData::Ed25519(_) => entry
                .private_key
                .try_sign(data)
                .map_err(|e| SshAgentError::SignFailed(e.to_string())),
            KeypairData::Rsa(keypair) => {
                let hash = hash.unwrap_or(HashAlg::Sha256);
                let component = |mpint: &Mpint| {
                    mpint
                        .as_positive_bytes()
                        .map(rsa::BigUint::from_bytes_be)
                        .ok_or_else(|| {
                            SshAgentError::SignFailed(
                                "rsa key component is not a positive integer".to_string(),
                            )
                        })
                };
                let private_key = rsa::RsaPrivateKey::from_components(
                    component(&keypair.public.n)?,
                    component(&keypair.public.e)?,
                    component(&keypair.private.d)?,
                    vec![
                        component(&keypair.private.p)?,
                        component(&keypair.private.q)?,
                    ],
                )
                .map_err(|e| SshAgentError::SignFailed(e.to_string()))?;
                let signature = match hash {
                    HashAlg::Sha256 => rsa::pkcs1v15::SigningKey::<Sha256>::new(private_key)
                        .try_sign(data)
                        .map_err(|e| SshAgentError::SignFailed(e.to_string()))?,
                    HashAlg::Sha512 => rsa::pkcs1v15::SigningKey::<Sha512>::new(private_key)
                        .try_sign(data)
                        .map_err(|e| SshAgentError::SignFailed(e.to_string()))?,
                    _ => {
                        return Err(SshAgentError::SignFailed(
                            "no rsa signing arm for the requested hash".to_string(),
                        ))
                    }
                };
                Signature::new(Algorithm::Rsa { hash: Some(hash) }, signature.to_vec())
                    .map_err(|e| SshAgentError::SignFailed(e.to_string()))
            }
            _ => Err(SshAgentError::SignFailed(
                "the cached key is outside the agent support matrix".to_string(),
            )),
        }
    }

    /// Warning reason for a key outside the support matrix.
    ///
    /// Names the algorithm with the label the entry field carries (lowercased),
    /// so a user can match the line against the entry they opted in.
    fn unsupported_algorithm_reason(algorithm: Algorithm) -> String {
        format!(
            "algorithm {} is not in the agent support matrix",
            key_type_name(algorithm).to_lowercase()
        )
    }

    /// Message for an opt-in the support matrix cannot serve.
    ///
    /// `bogita entry serve <name> --on` prints it: names the entry, the
    /// algorithm, and the shipped matrix. The matrix text comes from
    /// [`ALGORITHM_MATRIX`], so the message and the docs cannot drift apart.
    pub fn unsupported_algorithm_message(entry_name: &str, algorithm: Algorithm) -> String {
        format!(
            "entry '{entry_name}': algorithm {} is not supported yet; the agent can serve: \
             {ALGORITHM_MATRIX}",
            key_type_name(algorithm)
        )
    }
}

impl Drop for KeyRing {
    fn drop(&mut self) {
        self.clear();
    }
}

// endregion: --- Ring

// region:    --- Support

/// The shipped serve matrix, in one sentence.
///
/// README and AGENTS quote this line verbatim, and
/// [`KeyRing::unsupported_algorithm_message`] prints it, so the docs and the
/// refusal cannot drift apart. Widen it in the same commit as
/// [`algorithm_supported`].
pub const ALGORITHM_MATRIX: &str =
    "Ed25519 and RSA (rsa-sha2-256/512); ECDSA unsupported — roadmap";

/// Bit a SIGN_REQUEST sets to ask for an rsa-sha2-512 reply (RFC 9987 §8.3).
const RSA_SHA2_512_FLAG: u32 = 0x0000_0004;

/// Map the SIGN_REQUEST flags onto the digest an RSA reply must use.
///
/// RFC 9987 §8.3 reserves one bit per hash: `0x00000002` asks for
/// rsa-sha2-256 and `0x00000004` asks for rsa-sha2-512. A client that sets
/// neither bit gets the modern default, rsa-sha2-256: plain `ssh-rsa` means
/// SHA-1, which OpenSSH has deprecated. A client that sets both bits gets 512.
///
/// Ed25519 replies ignore the value: their algorithm follows the key.
pub fn rsa_hash_alg(flags: u32) -> Option<HashAlg> {
    Some(if flags & RSA_SHA2_512_FLAG != 0 {
        HashAlg::Sha512
    } else {
        HashAlg::Sha256
    })
}

/// True when the agent can serve a signature with this key type.
///
/// This function owns the shipped matrix: `Ed25519` and `Rsa` are served;
/// `Ecdsa`, `Dsa`, and `Other` are unsupported, and an ECDSA serve arm stays on
/// the roadmap. [`KeyRing::sign`] dispatches per algorithm, and `ssh-key` is
/// built with the `ed25519` and `rsa` features, so a key outside this matrix has
/// no signing arm.
pub fn algorithm_supported(algorithm: Algorithm) -> bool {
    matches!(algorithm, Algorithm::Ed25519 | Algorithm::Rsa { .. })
}

/// Report a skipped key by entry name and reason.
///
/// The reason never carries key material, and the name is the vault entry name the
/// user sees. The line goes to stderr through the descriptor, not through
/// `eprintln!`: the test harness sinks print-macro output in a thread-local buffer,
/// so only a descriptor write keeps the warning observable to the caller.
fn skip(entry: &Entry, reason: &str) {
    let _ = writeln!(
        std::io::stderr(),
        "warning: agent cannot use the key in entry '{}': {reason}",
        entry.name
    );
}

/// True when the entry carries the plaintext `agent_servable = true` gate.
fn is_agent_servable(entry: &Entry) -> bool {
    entry.fields.iter().any(|field| {
        let gated = field.key == AGENT_SERVABLE_KEY
            || field.field_type == FieldType::Custom(AGENT_SERVABLE_KEY.to_string());
        gated && matches!(field.value, FieldValue::Boolean(true))
    })
}

/// The stored OpenSSH private key PEM, if the entry has one.
fn private_pem(entry: &Entry) -> Option<String> {
    entry
        .fields
        .iter()
        .find(|field| field.field_type == FieldType::SshPrivateKey)
        .map(|field| field.value.to_string())
}

/// The `SshComment` field value, if the entry has a non-empty one.
fn ssh_comment(entry: &Entry) -> Option<String> {
    entry
        .fields
        .iter()
        .find(|field| field.field_type == FieldType::SshComment)
        .map(|field| field.value.to_string())
        .filter(|comment| !comment.is_empty())
}

// endregion: --- Support

// region:    --- Tests
#[cfg(test)]
#[path = "../../test/agent/keyring.rs"]
mod keyring_test;

#[cfg(test)]
#[path = "../../test/agent/keyring_rsa.rs"]
mod keyring_rsa_test;
// endregion: --- Tests
