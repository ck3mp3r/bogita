//! Keychain-presence lock watcher for the agent daemon.
//!
//! The daemon is a READ-ONLY consumer of the staged age identity: it never stages
//! and never deletes the keychain item. Unlock stages the raw identity in the OS
//! keychain item (the TUI or `bogita unlock` does that); lock deletes the item. The
//! item's presence therefore IS the lock signal, and this watcher polls it every
//! [`POLL_INTERVAL`]:
//!
//! | Keychain item | Watcher event | Ring |
//! |---------------|---------------|------|
//! | present | [`LockEvent::Unlocked`] | rebuilt from the vault |
//! | absent | [`LockEvent::Locked`] | cleared, rebuilds refused |
//! | present, idle longer than `lock_timeout` | [`LockEvent::Expired`] | cleared, rebuilds refused |
//!
//! # Re-population tradeoff
//!
//! When the item reappears, the ring is rebuilt automatically. The staged raw
//! identity in the keychain is already the unlocked-state exposure: anyone who can
//! read that item can open the vault without this daemon. Reading it adds no new
//! surface, and refusing to re-populate would leave the agent dead after every
//! unlock until a manual restart. The rule is documented in `AGENTS.md`.
//!
//! # Inactivity residency
//!
//! `lock_timeout = Some(n)` is a residency cap: after `n` seconds without a served
//! request the ring is evicted, exactly as on lock. The cap cannot be defeated by
//! request-driven re-population because the eviction is only cleared after the
//! watcher observes the full cycle -- item absent (`Locked`) and then present again
//! (`Unlocked`). A keychain item that simply stays present keeps the ring evicted,
//! so no request can revive it. `lock_timeout = None` never evicts on inactivity.
//!
//! # In-flight requests
//!
//! Eviction takes the same ring mutex that a request holds while it runs, so a
//! signature that started before the eviction completes and returns; requests that
//! arrive after the eviction see a locked ring and answer FAILURE(5). Detection is
//! at most one [`POLL_INTERVAL`] late.

use crate::agent::keyring::{KeyRing, SharedKeyRing};
use crate::domain::{AgeIdentity, Entry};
use crate::error::Result;
use crate::ports::{EntryStore, KeychainStore, VaultStore};
use crate::session::Session;
use crate::vault::registry::VaultRegistry;
use async_trait::async_trait;
use std::sync::MutexGuard;
use std::time::{Duration, Instant};
use tokio::time::{interval, MissedTickBehavior};

// region:    --- Protocol

/// Interval between two keychain reads. Lock and unlock are detected within this
/// window, which is the documented worst-case latency.
pub const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// One state change reported by [`LockWatcher`].
///
/// The variant carries what the ring needs: a rebuild needs the identity that is
/// currently staged.
pub enum LockEvent {
    /// The identity item disappeared: the vault is locked.
    Locked,
    /// The identity item is present: rebuild the ring from the vault.
    Unlocked(AgeIdentity),
    /// The inactivity cap elapsed while unlocked: evict the ring.
    Expired,
}

impl std::fmt::Debug for LockEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Locked => f.write_str("Locked"),
            Self::Unlocked(_) => f.write_str("Unlocked(<identity>)"),
            Self::Expired => f.write_str("Expired"),
        }
    }
}

// endregion: --- Protocol

// region:    --- Watcher

/// Last presence the watcher observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Presence {
    /// Nothing observed yet: the next read is a transition.
    Unknown,
    Present,
    Absent,
}

/// Polls the staged keychain identity and reports lock-state transitions.
///
/// A transition is reported once. Repeating the same observation reports nothing,
/// which is what makes the inactivity cap defeat-proof: while the item stays
/// present after [`LockEvent::Expired`], no further [`LockEvent::Unlocked`] is
/// produced until the item is absent first.
pub struct LockWatcher<K: KeychainStore> {
    session: Session<K>,
    timeout: Option<Duration>,
    presence: Presence,
    evicted: bool,
}

impl<K: KeychainStore> LockWatcher<K> {
    /// Watch the identity staged behind `session`.
    ///
    /// `lock_timeout` is the vault's inactivity cap in seconds; `None` disables
    /// inactivity eviction.
    pub fn new(session: Session<K>, lock_timeout: Option<u64>) -> Self {
        Self {
            session,
            timeout: lock_timeout.map(Duration::from_secs),
            presence: Presence::Unknown,
            evicted: false,
        }
    }

    /// Read the keychain once and return the transition, if any.
    ///
    /// `now` and `last_served` are injected so tests drive time without sleeping.
    /// `last_served` is the instant of the last successful request, or `None` while
    /// the ring holds no cache.
    pub fn poll_once(&mut self, now: Instant, last_served: Option<Instant>) -> Option<LockEvent> {
        match self.read_identity() {
            Some(identity) => self.present(identity, now, last_served),
            None => self.absent(),
        }
    }

    /// The staged identity, or `None` when the item is absent or unreadable.
    ///
    /// An unreadable item counts as absent: a keychain that cannot be read must not
    /// keep keys in the ring. The failure is reported without any secret material.
    fn read_identity(&self) -> Option<AgeIdentity> {
        match self.session.get_identity() {
            Ok(identity) => identity,
            Err(e) => {
                eprintln!("warning: agent cannot read the keychain item: {e}");
                None
            }
        }
    }

    fn present(
        &mut self,
        identity: AgeIdentity,
        now: Instant,
        last_served: Option<Instant>,
    ) -> Option<LockEvent> {
        let fresh = self.presence != Presence::Present;
        self.presence = Presence::Present;
        if fresh {
            self.evicted = false;
            return Some(LockEvent::Unlocked(identity));
        }
        if self.evicted {
            return None;
        }
        if self.inactive(now, last_served) {
            self.evicted = true;
            return Some(LockEvent::Expired);
        }
        None
    }

    fn absent(&mut self) -> Option<LockEvent> {
        let was_absent = self.presence == Presence::Absent;
        self.presence = Presence::Absent;
        self.evicted = false;
        if was_absent {
            None
        } else {
            Some(LockEvent::Locked)
        }
    }

    /// True when the vault is unlocked and the inactivity cap has elapsed.
    fn inactive(&self, now: Instant, last_served: Option<Instant>) -> bool {
        match (self.timeout, last_served) {
            (Some(limit), Some(last)) => now.saturating_duration_since(last) > limit,
            _ => false,
        }
    }
}

// endregion: --- Watcher

// region:    --- Source

/// Where the ring gets its keys when the vault unlocks.
///
/// The daemon holds the vault registry, not the raw storage adapter, so the
/// rebuild reads through the same ports the TUI uses. Static dispatch only.
#[async_trait]
pub trait UnlockSource: Send + Sync {
    /// Every entry of every vault, decrypted through `identity`.
    async fn entries(&self, identity: &AgeIdentity) -> Result<Vec<Entry>>;
}

#[async_trait]
impl<S> UnlockSource for VaultRegistry<S>
where
    S: VaultStore + EntryStore,
{
    async fn entries(&self, identity: &AgeIdentity) -> Result<Vec<Entry>> {
        let mut entries = Vec::new();
        for vault in self.list_vaults().await? {
            let service = self.vault_service_for(&vault, identity.clone());
            entries.extend(service.list_entries(vault.id, None).await?);
        }
        Ok(entries)
    }
}

// endregion: --- Source

// region:    --- Monitor

/// Applies watcher transitions to the shared ring.
///
/// This is the daemon's lock-state machine: one `step` reads the keychain and
/// performs the matching ring mutation. `step` takes an explicit clock so unit
/// tests are deterministic; [`LockMonitor::run`] drives it from a tokio interval.
pub struct LockMonitor<K: KeychainStore, U: UnlockSource> {
    watcher: LockWatcher<K>,
    ring: SharedKeyRing,
    source: U,
    period: Duration,
}

impl<K: KeychainStore, U: UnlockSource> LockMonitor<K, U> {
    /// Watch `session` and keep `ring` in step with the staged identity.
    pub fn new(
        session: Session<K>,
        lock_timeout: Option<u64>,
        ring: SharedKeyRing,
        source: U,
    ) -> Self {
        Self {
            watcher: LockWatcher::new(session, lock_timeout),
            ring,
            source,
            period: POLL_INTERVAL,
        }
    }

    /// Poll once and apply the transition, returning it.
    pub async fn step(&mut self, now: Instant) -> Option<LockEvent> {
        let last_served = self.ring().last_served_request();
        let event = self.watcher.poll_once(now, last_served)?;
        self.apply(&event).await;
        Some(event)
    }

    async fn apply(&mut self, event: &LockEvent) {
        match event {
            LockEvent::Locked | LockEvent::Expired => {
                let mut ring = self.ring();
                ring.clear();
                ring.forbid_rebuild();
            }
            LockEvent::Unlocked(identity) => {
                let entries = match self.source.entries(identity).await {
                    Ok(entries) => entries,
                    Err(e) => {
                        eprintln!("warning: agent cannot rebuild its key ring: {e}");
                        return;
                    }
                };
                let mut ring = self.ring();
                ring.allow_rebuild();
                ring.rebuild(entries);
            }
        }
    }

    fn ring(&self) -> MutexGuard<'_, KeyRing> {
        self.ring.lock().expect("key ring mutex poisoned")
    }
}

impl<K, U> LockMonitor<K, U>
where
    K: KeychainStore + Send + 'static,
    U: UnlockSource + 'static,
{
    /// Poll every [`POLL_INTERVAL`] until the task is aborted.
    pub async fn run(mut self) {
        let mut ticks = interval(self.period);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticks.tick().await;
            self.step(Instant::now()).await;
        }
    }
}

// endregion: --- Monitor

// region:    --- Tests
#[cfg(test)]
#[path = "../../test/agent/watcher.rs"]
mod watcher_test;
// endregion: --- Tests
