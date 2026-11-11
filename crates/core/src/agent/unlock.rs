//! The vault service: one owner of the lock and the vault registry.
//!
//! The service holds the registry and, while unlocked, the identity derived from
//! the encrypted identity file: entry reads decrypt through it and entry writes
//! encrypt with the vault recipients. `unlock` also rebuilds the agent's key ring,
//! so the daemon and the TUI drive the same unlock; `lock` drops the identity and
//! evicts the ring. No keychain, no external presence check.

use crate::agent::keyring::{KeyRing, SharedKeyRing};
use crate::agent::source::UnlockSource;
use crate::domain::{AgeIdentity, AgeRecipient, Entry, SecretString, Vault};
use crate::error::{Result, SessionError};
use crate::ports::IdentityStore;
use crate::vault::registry::VaultAccess;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::time::{interval, MissedTickBehavior};
use uuid::Uuid;

/// Interval between two idle-watch polls: the worst-case delay before an
/// eviction is noticed.
pub const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// The vault service a frontend drives: the lock state and the registry.
#[async_trait]
pub trait VaultService: Send + Sync {
    /// Derive the identity, rebuild the ring from the vault, and keep the identity
    /// as the service's unlocked state.
    ///
    /// Returns the number of keys now served. Any error leaves the service as it
    /// was; the request answers FAILURE without echoing anything.
    async fn unlock(&self, passphrase: &SecretString) -> Result<usize>;

    /// Evict the ring and drop the identity. Idempotent.
    fn lock(&self);

    /// True while no identity is held.
    fn is_locked(&self) -> bool;

    /// The public recipient of the held identity, or `None` while locked.
    fn recipient(&self) -> Option<AgeRecipient>;

    /// Every persisted vault.
    async fn list_vaults(&self) -> Result<Vec<Vault>>;

    /// Persist a new vault (or update an existing one).
    async fn add_vault(&self, vault: &Vault) -> Result<()>;

    /// Remove a vault by ID.
    async fn remove_vault(&self, id: Uuid) -> Result<()>;

    /// Make `id` the default vault, clearing any previous default.
    async fn set_default(&self, id: Uuid) -> Result<()>;

    /// Every entry of every vault. Empty while locked.
    async fn all_entries(&self) -> Result<Vec<Entry>>;

    /// Every entry of one vault, optionally filtered by a search query.
    async fn list_entries(&self, vault_id: Uuid, query: Option<&str>) -> Result<Vec<Entry>>;

    /// One entry by ID.
    async fn get_entry(&self, id: Uuid) -> Result<Option<Entry>>;

    /// Persist an entry, encrypting its fields with its vault's recipients.
    async fn save_entry(&self, entry: &Entry) -> Result<()>;

    /// Delete an entry by ID.
    async fn delete_entry(&self, id: Uuid) -> Result<()>;
}

/// A daemon with no identity source: every unlock is refused.
///
/// This keeps the protocol surface usable where no identity file is wired in.
#[derive(Default)]
pub struct NoLock;

#[async_trait]
impl VaultService for NoLock {
    async fn unlock(&self, _passphrase: &SecretString) -> Result<usize> {
        Err(SessionError::Locked.into())
    }

    fn lock(&self) {}

    fn is_locked(&self) -> bool {
        true
    }

    fn recipient(&self) -> Option<AgeRecipient> {
        None
    }

    async fn list_vaults(&self) -> Result<Vec<Vault>> {
        Err(SessionError::Locked.into())
    }

    async fn add_vault(&self, _vault: &Vault) -> Result<()> {
        Err(SessionError::Locked.into())
    }

    async fn remove_vault(&self, _id: Uuid) -> Result<()> {
        Err(SessionError::Locked.into())
    }

    async fn set_default(&self, _id: Uuid) -> Result<()> {
        Err(SessionError::Locked.into())
    }

    async fn all_entries(&self) -> Result<Vec<Entry>> {
        Ok(Vec::new())
    }

    async fn list_entries(&self, _vault_id: Uuid, _query: Option<&str>) -> Result<Vec<Entry>> {
        Err(SessionError::Locked.into())
    }

    async fn get_entry(&self, _id: Uuid) -> Result<Option<Entry>> {
        Err(SessionError::Locked.into())
    }

    async fn save_entry(&self, _entry: &Entry) -> Result<()> {
        Err(SessionError::Locked.into())
    }

    async fn delete_entry(&self, _id: Uuid) -> Result<()> {
        Err(SessionError::Locked.into())
    }
}

/// The real service: identity from the encrypted file, keys and entries through
/// the registry.
pub struct VaultLock<I, U> {
    identity_path: PathBuf,
    identity_store: I,
    source: U,
    ring: SharedKeyRing,
    identity: Mutex<Option<AgeIdentity>>,
}

impl<I, U> VaultLock<I, U> {
    /// Build the service for `ring`.
    pub fn new(
        identity_path: impl Into<PathBuf>,
        identity_store: I,
        source: U,
        ring: SharedKeyRing,
    ) -> Self {
        Self {
            identity_path: identity_path.into(),
            identity_store,
            source,
            ring,
            identity: Mutex::new(None),
        }
    }

    /// Seed an already-derived identity: the boot path that finds a staged one.
    pub fn with_identity(self, identity: AgeIdentity) -> Self {
        *self.identity() = Some(identity);
        self
    }

    fn ring(&self) -> MutexGuard<'_, KeyRing> {
        self.ring.lock().expect("key ring mutex poisoned")
    }

    fn identity(&self) -> MutexGuard<'_, Option<AgeIdentity>> {
        self.identity.lock().expect("identity mutex poisoned")
    }

    /// The held identity cloned out, so no guard crosses an await point.
    fn held_identity(&self) -> Option<AgeIdentity> {
        self.identity().clone()
    }

    /// The held identity, or the typed refusal a locked service answers.
    fn unlocked(&self) -> Result<AgeIdentity> {
        self.held_identity()
            .ok_or_else(|| SessionError::Locked.into())
    }
}

#[async_trait]
impl<I, U> VaultService for VaultLock<I, U>
where
    I: IdentityStore + Send + Sync,
    U: UnlockSource + VaultAccess,
{
    async fn unlock(&self, passphrase: &SecretString) -> Result<usize> {
        let identity = self
            .identity_store
            .read_encrypted(&self.identity_path, passphrase)?;
        let entries = self.source.entries(&identity).await?;

        let served = {
            let mut ring = self.ring();
            ring.allow_rebuild();
            if !ring.rebuild(entries) {
                return Err(SessionError::Locked.into());
            }
            ring.len()
        };

        *self.identity() = Some(identity);
        Ok(served)
    }

    fn lock(&self) {
        {
            let mut ring = self.ring();
            ring.clear();
            ring.forbid_rebuild();
        }
        *self.identity() = None;
    }

    fn is_locked(&self) -> bool {
        self.identity().is_none()
    }

    fn recipient(&self) -> Option<AgeRecipient> {
        self.held_identity().map(|identity| identity.to_recipient())
    }

    async fn list_vaults(&self) -> Result<Vec<Vault>> {
        self.source.list_vaults().await
    }

    async fn add_vault(&self, vault: &Vault) -> Result<()> {
        self.source.add_vault(vault).await
    }

    async fn remove_vault(&self, id: Uuid) -> Result<()> {
        self.source.remove_vault(id).await
    }

    async fn set_default(&self, id: Uuid) -> Result<()> {
        self.source.set_default(id).await
    }

    async fn all_entries(&self) -> Result<Vec<Entry>> {
        let Some(identity) = self.held_identity() else {
            return Ok(Vec::new());
        };
        self.source.entries(&identity).await
    }

    async fn list_entries(&self, vault_id: Uuid, query: Option<&str>) -> Result<Vec<Entry>> {
        let identity = self.unlocked()?;
        self.source.list_entries(vault_id, query, &identity).await
    }

    async fn get_entry(&self, id: Uuid) -> Result<Option<Entry>> {
        let identity = self.unlocked()?;
        self.source.get_entry(id, &identity).await
    }

    async fn save_entry(&self, entry: &Entry) -> Result<()> {
        let identity = self.unlocked()?;
        self.source.save_entry(entry, &identity).await
    }

    async fn delete_entry(&self, id: Uuid) -> Result<()> {
        self.source.delete_entry(id).await
    }
}

/// The daemon's own inactivity cap over the lock it serves.
///
/// Eviction is the service's lock, not a ring-only reset: an idle-evicted service
/// holds no identity and serves no key, exactly as after a lock request. It is
/// final until the next unlock request, so a request storm cannot revive an
/// evicted ring.
pub struct IdleWatch<S: VaultService> {
    timeout: Option<Duration>,
    ring: SharedKeyRing,
    service: Arc<S>,
}

impl<S: VaultService> IdleWatch<S> {
    /// Watch `service`, locking it after `lock_timeout` seconds without a served
    /// request. `None` never evicts on idleness.
    pub fn new(lock_timeout: Option<u64>, ring: SharedKeyRing, service: Arc<S>) -> Self {
        Self {
            timeout: lock_timeout.map(Duration::from_secs),
            ring,
            service,
        }
    }

    /// One poll at `now`: locks when the cap elapsed. Returns true when it did.
    pub fn step(&self, now: Instant) -> bool {
        let Some(limit) = self.timeout else {
            return false;
        };
        let last = {
            let ring = self.ring.lock().expect("key ring mutex poisoned");
            if ring.is_locked() {
                return false;
            }
            let Some(last) = ring.last_served_request() else {
                return false;
            };
            last
        };
        if now.saturating_duration_since(last) <= limit {
            return false;
        }
        self.service.lock();
        true
    }
}

impl<S: VaultService + 'static> IdleWatch<S> {
    /// Poll until the task is aborted.
    pub async fn run(self) {
        let mut ticks = interval(POLL_INTERVAL);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticks.tick().await;
            self.step(Instant::now());
        }
    }
}

// region:    --- Tests
#[cfg(test)]
#[path = "../../test/agent/unlock.rs"]
mod unlock_test;
// endregion: --- Tests
