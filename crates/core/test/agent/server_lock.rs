//! End-to-end tests for the vault interlock (agent/server.rs + agent/watcher.rs)
//!
//! Gated on the `integration` feature: these tests bind a real unix socket, open a
//! real SQLite database, and let the watcher poll a real 500 ms interval.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::error::SshAgentError;
use crate::agent::keyring::{KeyRing, SharedKeyRing};
use crate::agent::server::serve;
use crate::agent::watcher::LockMonitor;
use crate::crypto::age::AgeCrypto;
use crate::domain::ssh::{agent_servable_field, SshKeyMaterial};
use crate::domain::{AgeIdentity, Entry};
use crate::ports::EntryStore;
use crate::session::Session;
use crate::storage::sqlite::SqliteStorage;
use crate::test_support::{seed_vault, test_entry, MockKeychain};
use crate::vault::registry::VaultRegistry;
use signature::Verifier;
use ssh_agent_lib::agent::Session as AgentSession;
use ssh_agent_lib::client::Client;
use ssh_agent_lib::proto::{Extension, PublicCredential, Request, Response, SignRequest};
use ssh_key::public::KeyData;
use ssh_key::PublicKey;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use uuid::Uuid;

const DATA: &[u8] = b"ssh-agent-sign-request-data";
const STARTUP_LIMIT: usize = 200;
const STARTUP_INTERVAL: Duration = Duration::from_millis(10);

/// Detection budget from the criteria: lock and unlock are visible within 1000 ms.
const DETECTION_LIMIT: usize = 100;

/// One polling interval plus a margin: the documented worst-case latency.
const DETECTION: Duration = Duration::from_millis(600);

// region:    --- Fixtures

fn ssh_entry(name: &str, vault_id: Uuid, material: &SshKeyMaterial) -> Entry {
    let mut fields = material.to_fields();
    fields.push(agent_servable_field(true, fields.len() as i32));
    test_entry(vault_id, name, fields)
}

fn key_data(material: &SshKeyMaterial) -> Result<KeyData> {
    Ok(PublicKey::from_openssh(material.public_line())?
        .key_data()
        .clone())
}

fn sign_request(material: &SshKeyMaterial) -> Result<Request> {
    Ok(Request::SignRequest(SignRequest {
        credential: PublicCredential::Key(key_data(material)?),
        data: DATA.to_vec(),
        flags: 0,
    }))
}

fn status_request() -> Request {
    Request::Extension(Extension {
        name: "bogita-status@bogita".to_string(),
        details: Vec::new().into(),
    })
}

fn status_text(response: &Response) -> Result<String> {
    let Response::ExtensionResponse(reply) = response else {
        return Err(format!("expected an extension response, got {response:?}").into());
    };
    Ok(std::str::from_utf8(reply.details.as_ref())?.to_string())
}

/// Vault registry with one servable ed25519 entry, plus its identity and key.
async fn seeded_source(
    dir: &Path,
) -> Result<(
    VaultRegistry<SqliteStorage<AgeCrypto>>,
    AgeIdentity,
    SshKeyMaterial,
)> {
    let storage = SqliteStorage::new(dir.join("vault.db"), AgeCrypto).await?;
    let identity = AgeIdentity::generate();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await?;
    let material = SshKeyMaterial::keygen_ed25519("agent@example.com")?;
    storage
        .save_entry(
            &ssh_entry("github", vault_id, &material),
            &[identity.to_recipient()],
        )
        .await?;
    Ok((VaultRegistry::new(storage), identity, material))
}

/// The daemon plus its lock watcher, running on a real socket.
struct WatchedAgent {
    serving: JoinHandle<std::result::Result<(), SshAgentError>>,
    watching: JoinHandle<()>,
    socket_path: PathBuf,
    ring: SharedKeyRing,
}

impl WatchedAgent {
    async fn start(
        dir: &Path,
        keychain: &MockKeychain,
        identity: &AgeIdentity,
        lock_timeout: Option<u64>,
        source: VaultRegistry<SqliteStorage<AgeCrypto>>,
    ) -> Result<Self> {
        let socket_path = dir.join("data").join("agent.sock");
        Session::new(keychain.clone()).store_identity(identity)?;
        let ring: SharedKeyRing = Arc::new(Mutex::new(KeyRing::new()));
        let monitor = LockMonitor::new(
            Session::new(keychain.clone()),
            lock_timeout,
            Arc::clone(&ring),
            source,
        );
        let serving = tokio::spawn(serve(Arc::clone(&ring), socket_path.clone()));
        let watching = tokio::spawn(monitor.run());
        wait_for_socket(&socket_path).await?;
        Ok(Self {
            serving,
            watching,
            socket_path,
            ring,
        })
    }

    /// Wait until the watcher has loaded the ring from the vault, within the
    /// 1000 ms detection budget of the criteria.
    async fn wait_until_serving(&self, keys: usize) -> Result<()> {
        for _ in 0..DETECTION_LIMIT {
            if self.ring.lock().expect("key ring mutex poisoned").len() == keys {
                return Ok(());
            }
            tokio::time::sleep(STARTUP_INTERVAL).await;
        }
        Err(format!("the ring never reached {keys} keys within the detection budget").into())
    }

    async fn client(&self) -> Result<Client<UnixStream>> {
        Ok(Client::new(UnixStream::connect(&self.socket_path).await?))
    }

    async fn stop(self) {
        self.serving.abort();
        self.watching.abort();
        let _ = self.serving.await;
        let _ = self.watching.await;
    }
}

/// Block until the daemon has bound its socket (or the budget runs out).
async fn wait_for_socket(path: &Path) -> Result<()> {
    for _ in 0..STARTUP_LIMIT {
        if path.exists() {
            return Ok(());
        }
        tokio::time::sleep(STARTUP_INTERVAL).await;
    }
    Err(format!("agent socket {} never appeared", path.display()).into())
}

// endregion: --- Fixtures

// region:    --- Interlock (criterion 1, 2, 3, 4)

#[tokio::test]
async fn test_agent_server_lock_watcher_follows_the_keychain_item() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let keychain = MockKeychain::new();
    let (source, identity, material) = seeded_source(dir.path()).await?;
    let agent = WatchedAgent::start(dir.path(), &keychain, &identity, None, source).await?;
    agent.wait_until_serving(1).await?;
    let mut client = agent.client().await?;

    // -- Exec: unlocked
    let status_unlocked = client.handle(status_request()).await?;
    let listed_unlocked = client.handle(Request::RequestIdentities).await?;
    let signed_unlocked = client.handle(sign_request(&material)?).await?;

    // -- Check: the ring holds servable keys while the item exists
    let Response::IdentitiesAnswer(identities) = listed_unlocked else {
        return Err(format!("expected identities answer, got {listed_unlocked:?}").into());
    };
    assert_eq!(identities.len(), 1);
    assert_eq!(status_text(&status_unlocked)?, "1 keys, unlocked");
    let Response::SignResponse(signature) = signed_unlocked else {
        return Err(format!("expected a sign response, got {signed_unlocked:?}").into());
    };
    let public_key = PublicKey::from_openssh(material.public_line())?;
    Verifier::verify(&public_key, DATA, &signature)?;

    // -- Exec: lock the vault the way the CLI does
    Session::new(keychain.clone()).lock()?;
    tokio::time::sleep(DETECTION).await;
    let status_locked = client.handle(status_request()).await?;
    let listed_locked = client.handle(Request::RequestIdentities).await?;
    let signed_locked = client.handle(sign_request(&material)?).await?;

    // -- Check: the ring is evicted and every request fails
    assert!(agent
        .ring
        .lock()
        .expect("key ring mutex poisoned")
        .is_locked());
    assert_eq!(status_text(&status_locked)?, "0 keys, locked");
    assert_eq!(listed_locked, Response::Failure);
    assert_eq!(signed_locked, Response::Failure);

    // -- Exec: unlock again
    Session::new(keychain.clone()).store_identity(&identity)?;
    agent.wait_until_serving(1).await?;
    let status_reunlocked = client.handle(status_request()).await?;
    let listed_reunlocked = client.handle(Request::RequestIdentities).await?;

    // -- Check: the ring is rebuilt from the vault
    let Response::IdentitiesAnswer(identities) = listed_reunlocked else {
        return Err(format!("expected identities answer, got {listed_reunlocked:?}").into());
    };
    assert_eq!(identities.len(), 1);
    assert_eq!(status_text(&status_reunlocked)?, "1 keys, unlocked");

    agent.stop().await;
    Ok(())
}

// endregion: --- Interlock

// region:    --- Inactivity cap (criterion 5, 6)

#[tokio::test]
async fn test_agent_server_lock_watcher_evicts_after_the_inactivity_cap() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let keychain = MockKeychain::new();
    let (source, identity, material) = seeded_source(dir.path()).await?;
    let agent = WatchedAgent::start(dir.path(), &keychain, &identity, Some(1), source).await?;
    agent.wait_until_serving(1).await?;
    let mut client = agent.client().await?;

    // -- Exec: one served request, then silence past the cap
    let served = client.handle(sign_request(&material)?).await?;
    assert!(matches!(served, Response::SignResponse(_)));
    // The cap is 1 s; detection is at most one poll interval (500 ms) later.
    tokio::time::sleep(Duration::from_millis(1800)).await;
    let expired = client.handle(Request::RequestIdentities).await?;

    // -- Check: the ring is evicted even though the keychain item is still present
    assert_eq!(expired, Response::Failure);
    assert!(agent
        .ring
        .lock()
        .expect("key ring mutex poisoned")
        .is_locked());

    // -- Exec: keep talking to the daemon while the item stays present
    tokio::time::sleep(Duration::from_millis(700)).await;
    let defeated = client.handle(sign_request(&material)?).await?;

    // -- Check: requests cannot defeat the cap
    assert_eq!(
        defeated,
        Response::Failure,
        "a present item must not re-arm the evicted ring"
    );

    // -- Exec: full lock cycle, then unlock
    Session::new(keychain.clone()).lock()?;
    tokio::time::sleep(DETECTION).await;
    Session::new(keychain.clone()).store_identity(&identity)?;
    agent.wait_until_serving(1).await?;
    let rearmed = client.handle(Request::RequestIdentities).await?;

    // -- Check: a fresh unlock re-arms the agent
    let Response::IdentitiesAnswer(identities) = rearmed else {
        return Err(format!("expected identities answer, got {rearmed:?}").into());
    };
    assert_eq!(identities.len(), 1);

    agent.stop().await;
    Ok(())
}

#[tokio::test]
async fn test_agent_server_lock_watcher_keeps_serving_without_a_timeout() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let keychain = MockKeychain::new();
    let (source, identity, material) = seeded_source(dir.path()).await?;
    let agent = WatchedAgent::start(dir.path(), &keychain, &identity, None, source).await?;
    agent.wait_until_serving(1).await?;
    let mut client = agent.client().await?;

    // -- Exec
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let listed = client.handle(Request::RequestIdentities).await?;
    let signed = client.handle(sign_request(&material)?).await?;

    // -- Check: lock_timeout=None leaves the ring resident
    let Response::IdentitiesAnswer(identities) = listed else {
        return Err(format!("expected identities answer, got {listed:?}").into());
    };
    assert_eq!(identities.len(), 1);
    assert!(matches!(signed, Response::SignResponse(_)));

    agent.stop().await;
    Ok(())
}

// endregion: --- Inactivity cap
