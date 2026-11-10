//! Tests for the lock watcher and monitor (agent/watcher.rs)
//!
//! Pure: the keychain is `MockKeychain`, the clock is injected, and the entry source
//! is in memory. No database, no socket, no OS keychain, no sleeping.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::keyring::{KeyRing, SharedKeyRing};
use crate::agent::watcher::{LockEvent, LockMonitor, LockWatcher, UnlockSource};
use crate::domain::ssh::{agent_servable_field, SshKeyMaterial};
use crate::domain::{AgeIdentity, Entry};
use crate::error::{CryptoError, Result as CoreResult};
use crate::ports::KeychainStore;
use crate::session::Session;
use crate::test_support::{test_entry, MockKeychain};
use async_trait::async_trait;
use signature::Verifier;
use ssh_key::public::KeyData;
use ssh_key::PublicKey;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

const DATA: &[u8] = b"ssh-agent-sign-request-data";

// region:    --- Fixtures

/// Entry source backed by in-memory entries.
struct FakeSource(Vec<Entry>);

#[async_trait]
impl UnlockSource for FakeSource {
    async fn entries(&self, _identity: &AgeIdentity) -> CoreResult<Vec<Entry>> {
        Ok(self.0.clone())
    }
}

/// A keychain that always fails to read, as a locked keychain prompt would.
struct BrokenKeychain;

impl KeychainStore for BrokenKeychain {
    fn store_identity(&self, _secret_key: &str) -> CoreResult<()> {
        Ok(())
    }

    fn get_identity(&self) -> CoreResult<Option<String>> {
        Err(CryptoError::KeychainError("access denied".to_string()).into())
    }

    fn delete_identity(&self) -> CoreResult<()> {
        Ok(())
    }
}

fn ssh_entry(name: &str, material: &SshKeyMaterial, servable: bool) -> Entry {
    let mut fields = material.to_fields();
    fields.push(agent_servable_field(servable, fields.len() as i32));
    test_entry(Uuid::new_v4(), name, fields)
}

fn key_data(material: &SshKeyMaterial) -> Result<KeyData> {
    Ok(PublicKey::from_openssh(material.public_line())?
        .key_data()
        .clone())
}

fn ring_with(material: &SshKeyMaterial) -> SharedKeyRing {
    let mut ring = KeyRing::new();
    ring.load_entries(vec![ssh_entry("github", material, true)]);
    Arc::new(Mutex::new(ring))
}

/// A keychain with the identity staged and a watcher over it.
fn staged_watcher(
    keychain: &MockKeychain,
    identity: &AgeIdentity,
    lock_timeout: Option<u64>,
) -> Result<LockWatcher<MockKeychain>> {
    Session::new(keychain.clone()).store_identity(identity)?;
    Ok(LockWatcher::new(
        Session::new(keychain.clone()),
        lock_timeout,
    ))
}

/// A monitor over the same keychain, with a ring and an in-memory source.
fn staged_monitor(
    keychain: &MockKeychain,
    identity: &AgeIdentity,
    lock_timeout: Option<u64>,
    ring: SharedKeyRing,
    entries: Vec<Entry>,
) -> LockMonitor<MockKeychain, FakeSource> {
    Session::new(keychain.clone())
        .store_identity(identity)
        .expect("stage identity");
    LockMonitor::new(
        Session::new(keychain.clone()),
        lock_timeout,
        ring,
        FakeSource(entries),
    )
}

fn lock_ring(ring: &SharedKeyRing) -> std::sync::MutexGuard<'_, KeyRing> {
    ring.lock().expect("key ring mutex poisoned")
}

// endregion: --- Fixtures

// region:    --- Watcher transitions (criterion 1, 2, 4)

#[test]
fn test_agent_watcher_present_item_reports_unlocked() -> Result<()> {
    // -- Setup & Fixtures
    let keychain = MockKeychain::new();
    let identity = AgeIdentity::generate();
    let mut watcher = staged_watcher(&keychain, &identity, None)?;

    // -- Exec
    let event = watcher.poll_once(Instant::now(), None);

    // -- Check
    assert!(
        matches!(&event, Some(LockEvent::Unlocked(staged)) if staged.to_recipient() == identity.to_recipient()),
        "a staged identity reports unlocked, got {event:?}"
    );
    Ok(())
}

#[test]
fn test_agent_watcher_absent_item_reports_locked() -> Result<()> {
    // -- Setup & Fixtures
    let keychain = MockKeychain::new();
    let mut watcher = LockWatcher::new(Session::new(keychain), None);

    // -- Exec
    let event = watcher.poll_once(Instant::now(), None);

    // -- Check
    assert!(matches!(event, Some(LockEvent::Locked)), "got {event:?}");
    Ok(())
}

#[test]
fn test_agent_watcher_repeated_observation_reports_nothing() -> Result<()> {
    // -- Setup & Fixtures
    let keychain = MockKeychain::new();
    let identity = AgeIdentity::generate();
    let mut watcher = staged_watcher(&keychain, &identity, None)?;
    watcher.poll_once(Instant::now(), None);

    // -- Exec
    let event = watcher.poll_once(Instant::now(), None);

    // -- Check
    assert!(event.is_none(), "a steady state is not a transition");
    Ok(())
}

#[test]
fn test_agent_watcher_reappearing_item_reports_unlocked_again() -> Result<()> {
    // -- Setup & Fixtures
    let keychain = MockKeychain::new();
    let identity = AgeIdentity::generate();
    let mut watcher = staged_watcher(&keychain, &identity, None)?;
    watcher.poll_once(Instant::now(), None);
    Session::new(keychain.clone()).lock()?;
    let locked = watcher.poll_once(Instant::now(), None);
    assert!(matches!(locked, Some(LockEvent::Locked)));

    // -- Exec
    Session::new(keychain.clone()).store_identity(&identity)?;
    let event = watcher.poll_once(Instant::now(), None);

    // -- Check
    assert!(
        matches!(event, Some(LockEvent::Unlocked(_))),
        "a fresh unlock is a transition, got {event:?}"
    );
    Ok(())
}

#[test]
fn test_agent_watcher_unreadable_item_counts_as_locked() {
    // -- Setup & Fixtures
    let mut watcher = LockWatcher::new(Session::new(BrokenKeychain), None);

    // -- Exec
    let event = watcher.poll_once(Instant::now(), None);

    // -- Check
    assert!(
        matches!(event, Some(LockEvent::Locked)),
        "an unreadable keychain must not keep keys alive, got {event:?}"
    );
}

// endregion: --- Watcher transitions

// region:    --- Inactivity cap (criterion 5, 6)

#[test]
fn test_agent_watcher_expires_after_the_inactivity_cap() -> Result<()> {
    // -- Setup & Fixtures
    let keychain = MockKeychain::new();
    let identity = AgeIdentity::generate();
    let mut watcher = staged_watcher(&keychain, &identity, Some(30))?;
    let start = Instant::now();
    watcher.poll_once(start, Some(start));

    // -- Exec
    let event = watcher.poll_once(start + Duration::from_secs(31), Some(start));

    // -- Check
    assert!(matches!(event, Some(LockEvent::Expired)), "got {event:?}");
    Ok(())
}

#[test]
fn test_agent_watcher_keeps_serving_exactly_at_the_cap() -> Result<()> {
    // -- Setup & Fixtures
    let keychain = MockKeychain::new();
    let identity = AgeIdentity::generate();
    let mut watcher = staged_watcher(&keychain, &identity, Some(30))?;
    let start = Instant::now();
    watcher.poll_once(start, Some(start));

    // -- Exec
    let event = watcher.poll_once(start + Duration::from_secs(30), Some(start));

    // -- Check
    assert!(event.is_none(), "the cap evicts past n, not at n");
    Ok(())
}

#[test]
fn test_agent_watcher_stays_evicted_while_the_item_stays_present() -> Result<()> {
    // -- Setup & Fixtures
    let keychain = MockKeychain::new();
    let identity = AgeIdentity::generate();
    let mut watcher = staged_watcher(&keychain, &identity, Some(30))?;
    let start = Instant::now();
    watcher.poll_once(start, Some(start));
    let expired = watcher.poll_once(start + Duration::from_secs(31), Some(start));
    assert!(matches!(expired, Some(LockEvent::Expired)));

    // -- Exec
    let repeated = watcher.poll_once(start + Duration::from_secs(90), Some(start));

    // -- Check
    assert!(
        repeated.is_none(),
        "a present item alone must not re-populate the ring"
    );
    Ok(())
}

#[test]
fn test_agent_watcher_rearms_after_a_full_lock_cycle() -> Result<()> {
    // -- Setup & Fixtures
    let keychain = MockKeychain::new();
    let identity = AgeIdentity::generate();
    let mut watcher = staged_watcher(&keychain, &identity, Some(30))?;
    let start = Instant::now();
    watcher.poll_once(start, Some(start));
    watcher.poll_once(start + Duration::from_secs(31), Some(start));

    // -- Exec
    Session::new(keychain.clone()).lock()?;
    let locked = watcher.poll_once(start + Duration::from_secs(31), None);
    Session::new(keychain.clone()).store_identity(&identity)?;
    let unlocked = watcher.poll_once(start + Duration::from_secs(31), None);

    // -- Check
    assert!(matches!(locked, Some(LockEvent::Locked)));
    assert!(
        matches!(unlocked, Some(LockEvent::Unlocked(_))),
        "item absent then present re-arms the ring, got {unlocked:?}"
    );
    Ok(())
}

#[test]
fn test_agent_watcher_without_timeout_never_expires() -> Result<()> {
    // -- Setup & Fixtures
    let keychain = MockKeychain::new();
    let identity = AgeIdentity::generate();
    let mut watcher = staged_watcher(&keychain, &identity, None)?;
    let start = Instant::now();
    watcher.poll_once(start, Some(start));

    // -- Exec
    let event = watcher.poll_once(start + Duration::from_secs(86_400), Some(start));

    // -- Check
    assert!(event.is_none(), "lock_timeout=None has no residency cap");
    Ok(())
}

// endregion: --- Inactivity cap

// region:    --- Monitor actions (criterion 2, 4, 5, 7)

#[tokio::test]
async fn test_agent_monitor_unlock_rebuilds_the_ring() -> Result<()> {
    // -- Setup & Fixtures
    let keychain = MockKeychain::new();
    let identity = AgeIdentity::generate();
    let material = SshKeyMaterial::keygen_ed25519("unlock@example.com")?;
    let ring = Arc::new(Mutex::new(KeyRing::new()));
    let mut monitor = staged_monitor(
        &keychain,
        &identity,
        None,
        Arc::clone(&ring),
        vec![ssh_entry("github", &material, true)],
    );

    // -- Exec
    let event = monitor.step(Instant::now()).await;

    // -- Check
    assert!(matches!(event, Some(LockEvent::Unlocked(_))));
    assert_eq!(lock_ring(&ring).len(), 1, "the ring holds the servable key");
    assert!(lock_ring(&ring).rebuild_allowed());
    Ok(())
}

#[tokio::test]
async fn test_agent_monitor_lock_clears_the_ring_and_refuses_rebuild() -> Result<()> {
    // -- Setup & Fixtures
    let keychain = MockKeychain::new();
    let identity = AgeIdentity::generate();
    let material = SshKeyMaterial::keygen_ed25519("lock@example.com")?;
    let ring = ring_with(&material);
    let mut monitor = staged_monitor(
        &keychain,
        &identity,
        None,
        Arc::clone(&ring),
        vec![ssh_entry("github", &material, true)],
    );
    monitor.step(Instant::now()).await;
    Session::new(keychain.clone()).lock()?;

    // -- Exec
    let event = monitor.step(Instant::now()).await;

    // -- Check
    assert!(matches!(event, Some(LockEvent::Locked)));
    let mut ring = lock_ring(&ring);
    assert!(ring.is_locked());
    assert_eq!(ring.len(), 0);
    assert!(!ring.rebuild_allowed(), "a locked ring refuses rebuilds");
    assert!(!ring.rebuild(vec![ssh_entry("github", &material, true)]));
    assert!(
        ring.sign(&key_data(&material)?, DATA, None).is_err(),
        "a locked ring refuses signatures"
    );
    Ok(())
}

#[tokio::test]
async fn test_agent_monitor_expiry_keeps_the_ring_evicted_while_present() -> Result<()> {
    // -- Setup & Fixtures
    let keychain = MockKeychain::new();
    let identity = AgeIdentity::generate();
    let material = SshKeyMaterial::keygen_ed25519("idle@example.com")?;
    let ring = ring_with(&material);
    let mut monitor = staged_monitor(
        &keychain,
        &identity,
        Some(30),
        Arc::clone(&ring),
        vec![ssh_entry("github", &material, true)],
    );
    let start = Instant::now();
    monitor.step(start).await;

    // -- Exec
    let expired = monitor.step(start + Duration::from_secs(31)).await;
    let repeated = monitor.step(start + Duration::from_secs(90)).await;

    // -- Check
    assert!(matches!(expired, Some(LockEvent::Expired)));
    assert!(
        repeated.is_none(),
        "the item alone must not re-arm the ring"
    );
    let ring_guard = lock_ring(&ring);
    assert!(ring_guard.is_locked());
    assert!(!ring_guard.rebuild_allowed());
    Ok(())
}

#[tokio::test]
async fn test_agent_monitor_without_timeout_keeps_the_ring_after_idle() -> Result<()> {
    // -- Setup & Fixtures
    let keychain = MockKeychain::new();
    let identity = AgeIdentity::generate();
    let material = SshKeyMaterial::keygen_ed25519("resident@example.com")?;
    let ring = ring_with(&material);
    let mut monitor = staged_monitor(
        &keychain,
        &identity,
        None,
        Arc::clone(&ring),
        vec![ssh_entry("github", &material, true)],
    );
    let start = Instant::now();
    monitor.step(start).await;

    // -- Exec
    let event = monitor.step(start + Duration::from_secs(86_400)).await;

    // -- Check
    assert!(event.is_none(), "lock_timeout=None evicts nothing");
    assert_eq!(lock_ring(&ring).len(), 1);
    Ok(())
}

#[tokio::test]
async fn test_agent_monitor_in_flight_sign_completes_before_eviction() -> Result<()> {
    // -- Setup & Fixtures
    let keychain = MockKeychain::new();
    let identity = AgeIdentity::generate();
    let material = SshKeyMaterial::keygen_ed25519("inflight@example.com")?;
    let ring = Arc::new(Mutex::new(KeyRing::new()));
    let mut monitor = staged_monitor(
        &keychain,
        &identity,
        None,
        Arc::clone(&ring),
        vec![ssh_entry("github", &material, true)],
    );
    monitor.step(Instant::now()).await;

    // A request holds the ring while it signs; eviction needs the same lock.
    let held = Arc::clone(&ring);
    let key = key_data(&material)?;
    let (started, started_rx) = std::sync::mpsc::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let guard = held.lock().expect("key ring mutex poisoned");
        started.send(()).expect("signal the request start");
        release_rx.recv().expect("wait for the eviction window");
        let signature = guard.sign(&key, DATA, None);
        drop(guard);
        signature
    });
    started_rx.recv().expect("request started");
    assert!(
        ring.try_lock().is_err(),
        "eviction cannot take the ring while a request runs"
    );
    Session::new(keychain.clone()).lock()?;

    // -- Exec
    release.send(()).expect("release the in-flight request");
    let signature = worker.join().expect("the request thread panicked")?;
    let event = monitor.step(Instant::now()).await;

    // -- Check
    let public_key = PublicKey::from_openssh(material.public_line())?;
    Verifier::verify(&public_key, DATA, &signature)?;
    assert!(matches!(event, Some(LockEvent::Locked)));
    assert!(
        lock_ring(&ring).is_locked(),
        "the ring is evicted after it drains"
    );
    Ok(())
}

// endregion: --- Monitor actions
