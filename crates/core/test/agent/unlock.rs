//! The daemon-held lock: refusals, population, eviction and the wire names.
//!
//! The fixture keeps no SSH material: every assertion reads RING STATE, which is
//! what the lock actually controls.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::control::{LOCK_EXTENSION, UNLOCK_EXTENSION};
use crate::agent::keyring::{KeyRing, SharedKeyRing};
use crate::agent::source::UnlockSource;
use crate::agent::unlock::{IdleWatch, VaultLock, VaultService};
use crate::domain::{AgeIdentity, Entry, SecretString, Vault};
use crate::storage::identity::write_identity_encrypted;
use crate::test_support::test_entry;
use crate::vault::registry::VaultAccess;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use uuid::Uuid;

/// Identity source that ignores the identity and returns fixed entries.
///
/// The registry half answers as an empty store: these tests drive the ring only,
/// through `entries`.
struct FixedSource(Vec<Entry>);

#[async_trait]
impl UnlockSource for FixedSource {
    async fn entries(&self, _identity: &AgeIdentity) -> crate::error::Result<Vec<Entry>> {
        Ok(self.0.clone())
    }
}

#[async_trait]
impl VaultAccess for FixedSource {
    async fn list_vaults(&self) -> crate::error::Result<Vec<Vault>> {
        Ok(Vec::new())
    }

    async fn add_vault(&self, _vault: &Vault) -> crate::error::Result<()> {
        Ok(())
    }

    async fn remove_vault(&self, _id: Uuid) -> crate::error::Result<()> {
        Ok(())
    }

    async fn set_default(&self, _id: Uuid) -> crate::error::Result<()> {
        Ok(())
    }

    async fn list_entries(
        &self,
        _vault_id: Uuid,
        _query: Option<&str>,
        _identity: &AgeIdentity,
    ) -> crate::error::Result<Vec<Entry>> {
        Ok(Vec::new())
    }

    async fn get_entry(
        &self,
        _id: Uuid,
        _identity: &AgeIdentity,
    ) -> crate::error::Result<Option<Entry>> {
        Ok(None)
    }

    async fn save_entry(
        &self,
        _entry: &Entry,
        _identity: &AgeIdentity,
    ) -> crate::error::Result<()> {
        Ok(())
    }

    async fn delete_entry(&self, _id: Uuid) -> crate::error::Result<()> {
        Ok(())
    }
}

fn entries() -> Vec<Entry> {
    vec![test_entry(Uuid::new_v4(), "git", vec![])]
}

fn locked_ring() -> SharedKeyRing {
    Arc::new(Mutex::new(KeyRing::new()))
}

fn serving_ring() -> SharedKeyRing {
    let mut ring = KeyRing::new();
    ring.allow_rebuild();
    ring.rebuild(entries());
    Arc::new(Mutex::new(ring))
}

fn vault_lock(
    dir: &TempDir,
    ring: SharedKeyRing,
) -> Result<(
    VaultLock<crate::ports::EncryptedIdentityFile, FixedSource>,
    SecretString,
)> {
    let identity = AgeIdentity::generate();
    let passphrase = SecretString::from("test passphrase");
    let path = dir.path().join("identity.age");
    write_identity_encrypted(&identity, &passphrase, &path)?;
    let lock = VaultLock::new(
        path,
        crate::ports::EncryptedIdentityFile,
        FixedSource(entries()),
        ring,
    );
    Ok((lock, passphrase))
}

#[test]
fn test_unlock_and_lock_extensions_are_named() {
    assert_eq!(UNLOCK_EXTENSION, "bogita-unlock@bogita");
    assert_eq!(LOCK_EXTENSION, "bogita-lock@bogita");
}

#[tokio::test]
async fn test_wrong_passphrase_keeps_the_ring_locked() -> Result<()> {
    let dir = TempDir::new()?;
    let ring = locked_ring();
    let (lock, _right) = vault_lock(&dir, Arc::clone(&ring))?;

    let refused = lock.unlock(&SecretString::from("wrong")).await;

    assert!(refused.is_err(), "a wrong passphrase must be refused");
    assert!(
        ring.lock().expect("ring").is_locked(),
        "a refusal must leave the ring locked"
    );
    assert!(
        !ring.lock().expect("ring").rebuild_allowed(),
        "a refused unlock must leave the ring forbidden"
    );
    Ok(())
}

#[tokio::test]
async fn test_right_passphrase_populates_the_ring() -> Result<()> {
    let dir = TempDir::new()?;
    let ring = locked_ring();
    let (lock, passphrase) = vault_lock(&dir, Arc::clone(&ring))?;

    lock.unlock(&passphrase).await?;

    assert!(
        !ring.lock().expect("ring").is_locked(),
        "the right passphrase serves the vault"
    );
    Ok(())
}

#[tokio::test]
async fn test_lock_is_idempotent_and_requests_cannot_revive() -> Result<()> {
    let dir = TempDir::new()?;
    let ring = serving_ring();
    let (lock, passphrase) = vault_lock(&dir, Arc::clone(&ring))?;

    lock.lock();
    lock.lock();
    assert!(ring.lock().expect("ring").is_locked());

    for _ in 0..50 {
        let mut guard = ring.lock().expect("ring");
        assert!(
            !guard.rebuild(entries()),
            "a request storm must not rebuild an evicted ring"
        );
        guard.note_request();
    }
    assert!(ring.lock().expect("ring").is_locked());

    lock.unlock(&passphrase).await?;
    assert!(
        !ring.lock().expect("ring").is_locked(),
        "only a fresh unlock revives the ring"
    );
    Ok(())
}

#[test]
fn test_idle_watch_evicts_after_the_cap() -> Result<()> {
    let dir = TempDir::new()?;
    let ring = serving_ring();
    let (lock, _) = vault_lock(&dir, Arc::clone(&ring))?;
    let lock = Arc::new(lock);
    let watch = IdleWatch::new(Some(5), Arc::clone(&ring), Arc::clone(&lock));

    let served_at = {
        let mut r = ring.lock().expect("ring");
        r.note_request();
        r.last_served_request().expect("a served request")
    };

    assert!(
        !watch.step(served_at + Duration::from_secs(4)),
        "inside the cap"
    );
    assert!(
        watch.step(served_at + Duration::from_secs(6)),
        "past the cap"
    );
    assert!(ring.lock().expect("ring").is_locked());
    assert!(
        !ring.lock().expect("ring").rebuild(entries()),
        "an evicted ring refuses rebuilds"
    );

    let never = IdleWatch::new(None, Arc::clone(&ring), Arc::clone(&lock));
    assert!(!never.step(Instant::now() + Duration::from_secs(600)));
    Ok(())
}

#[tokio::test]
async fn test_idle_eviction_drops_the_identity_with_the_ring() -> Result<()> {
    // -- Setup & Fixtures: an unlocked service serving one entry
    let dir = TempDir::new()?;
    let ring = serving_ring();
    let (lock, _) = vault_lock(&dir, Arc::clone(&ring))?;
    let lock = Arc::new(lock.with_identity(AgeIdentity::generate()));
    let watch = IdleWatch::new(Some(5), Arc::clone(&ring), Arc::clone(&lock));
    let served_at = {
        let mut r = ring.lock().expect("ring");
        r.note_request();
        r.last_served_request().expect("a served request")
    };

    // -- Exec
    let evicted = watch.step(served_at + Duration::from_secs(6));

    // -- Check: idle-evicted means no identity in memory, exactly as lock
    assert!(evicted, "the cap must evict");
    assert!(
        lock.is_locked(),
        "idle eviction must drop the identity, not only the ring"
    );
    assert!(
        lock.recipient().is_none(),
        "an evicted service has no recipient"
    );
    assert!(ring.lock().expect("ring").is_locked());
    Ok(())
}
