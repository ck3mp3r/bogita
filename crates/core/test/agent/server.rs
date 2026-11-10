//! Integration tests for the agent serve loop (agent/server.rs)
//!
//! Gated on the `integration` feature: these tests bind real unix sockets and
//! open real SQLite databases.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::error::SshAgentError;
use crate::agent::keyring::KeyRing;
use crate::agent::server::{serve, SharedKeyRing};
use crate::agent::socket;
use crate::crypto::age::AgeCrypto;
use crate::domain::ssh::{agent_servable_field, SshKeyMaterial};
use crate::domain::{AgeIdentity, Entry, FieldType, FieldValue};
use crate::ports::EntryStore;
use crate::storage::config::ensure_private_dir;
use crate::storage::sqlite::SqliteStorage;
use crate::test_support::{seed_vault, test_entry};
use signature::Verifier;
use ssh_agent_lib::agent::Session;
use ssh_agent_lib::client::Client;
use ssh_agent_lib::proto::{Extension, PublicCredential, Request, Response, SignRequest};
use ssh_key::public::KeyData;
use ssh_key::PublicKey;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use uuid::Uuid;

const DATA: &[u8] = b"ssh-agent-sign-request-data";
const STARTUP_LIMIT: usize = 200;

// region:    --- Fixtures

fn ssh_entry(name: &str, vault_id: Uuid, material: &SshKeyMaterial, servable: bool) -> Entry {
    let mut fields = material.to_fields();
    fields.push(agent_servable_field(servable, fields.len() as i32));
    test_entry(vault_id, name, fields)
}

fn key_data(material: &SshKeyMaterial) -> Result<KeyData> {
    Ok(PublicKey::from_openssh(material.public_line())?
        .key_data()
        .clone())
}

fn locked_ring() -> SharedKeyRing {
    Arc::new(Mutex::new(KeyRing::new()))
}

fn mode_of(path: &Path) -> Result<u32> {
    Ok(std::fs::metadata(path)?.permissions().mode() & 0o777)
}

/// Block until the daemon has bound its socket (or the budget runs out).
async fn wait_for_socket(path: &Path) -> Result<()> {
    for _ in 0..STARTUP_LIMIT {
        if path.exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!("agent socket {} never appeared", path.display()).into())
}

struct RunningAgent {
    handle: JoinHandle<std::result::Result<(), SshAgentError>>,
    socket_path: PathBuf,
}

impl RunningAgent {
    async fn start(ring: SharedKeyRing, socket_path: PathBuf) -> Result<Self> {
        let handle = tokio::spawn(serve(ring, socket_path.clone()));
        wait_for_socket(&socket_path).await?;
        Ok(Self {
            handle,
            socket_path,
        })
    }

    async fn client(&self) -> Result<Client<UnixStream>> {
        Ok(Client::new(UnixStream::connect(&self.socket_path).await?))
    }

    async fn stop(self) {
        self.handle.abort();
        let _ = self.handle.await;
    }
}

async fn seeded_storage(dir: &Path) -> Result<(SqliteStorage<AgeCrypto>, AgeIdentity, Uuid)> {
    let storage = SqliteStorage::new(dir.join("vault.db"), AgeCrypto).await?;
    let identity = AgeIdentity::generate();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await?;
    Ok((storage, identity, vault_id))
}

fn extension(name: &str) -> Extension {
    Extension {
        name: name.to_string(),
        details: Vec::new().into(),
    }
}

// endregion: --- Fixtures

// region:    --- Socket lifecycle

#[test]
fn test_agent_server_refuses_when_socket_already_answers() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = dir.path().join("data").join("agent.sock");
    ensure_private_dir(socket_path.parent().expect("socket parent"))?;
    let live = std::os::unix::net::UnixListener::bind(&socket_path)?;

    // -- Exec
    let result = socket::prepare(&socket_path);

    // -- Check
    assert!(
        matches!(result, Err(SshAgentError::AlreadyServing(_))),
        "a live agent must not be replaced, got {result:?}"
    );
    drop(live);
    Ok(())
}

#[test]
fn test_agent_server_removes_stale_socket_file() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = dir.path().join("data").join("agent.sock");
    ensure_private_dir(socket_path.parent().expect("socket parent"))?;
    std::fs::write(&socket_path, b"stale")?;

    // -- Exec
    socket::prepare(&socket_path)?;

    // -- Check
    assert!(!socket_path.exists(), "the stale file is cleared");
    Ok(())
}

#[tokio::test]
async fn test_agent_server_binds_owner_only_socket() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let data_dir = dir.path().join("data");
    let socket_path = data_dir.join("agent.sock");
    let agent = RunningAgent::start(locked_ring(), socket_path.clone()).await?;

    // -- Check
    assert_eq!(mode_of(&data_dir)?, 0o700, "socket dir must be owner-only");
    assert_eq!(mode_of(&socket_path)?, 0o600, "socket must be owner-only");

    agent.stop().await;
    Ok(())
}

#[tokio::test]
async fn test_agent_server_unlinks_socket_when_serving_stops() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = dir.path().join("data").join("agent.sock");
    let agent = RunningAgent::start(locked_ring(), socket_path.clone()).await?;
    assert!(socket_path.exists());

    // -- Exec
    agent.stop().await;

    // -- Check
    assert!(!socket_path.exists(), "the socket file must be unlinked");
    Ok(())
}

// endregion: --- Socket lifecycle

// region:    --- Round trip

#[tokio::test]
async fn test_agent_server_roundtrips_identities_and_signature() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (storage, identity, vault_id) = seeded_storage(dir.path()).await?;
    let material = SshKeyMaterial::keygen_ed25519("agent@example.com")?;
    let hidden = SshKeyMaterial::keygen_ed25519("hidden@example.com")?;
    let recipients = vec![identity.to_recipient()];
    storage
        .save_entry(&ssh_entry("github", vault_id, &material, true), &recipients)
        .await?;
    storage
        .save_entry(&ssh_entry("private", vault_id, &hidden, false), &recipients)
        .await?;
    let mut ring = KeyRing::new();
    let loaded = ring.populate(&storage, &identity).await?;
    assert_eq!(loaded, 1, "only the servable entry reaches the ring");
    let agent = RunningAgent::start(
        Arc::new(Mutex::new(ring)),
        dir.path().join("data").join("agent.sock"),
    )
    .await?;
    let mut client = agent.client().await?;

    // -- Exec
    let identities = client.handle(Request::RequestIdentities).await?;
    let signed = client
        .handle(Request::SignRequest(SignRequest {
            credential: PublicCredential::Key(key_data(&material)?),
            data: DATA.to_vec(),
            flags: 0,
        }))
        .await?;

    // -- Check
    let Response::IdentitiesAnswer(identities) = identities else {
        return Err(format!("expected identities answer, got {identities:?}").into());
    };
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].comment, "agent@example.com");
    assert_eq!(identities[0].credential.key_data(), &key_data(&material)?);
    let Response::SignResponse(signature) = signed else {
        return Err(format!("expected sign response, got {signed:?}").into());
    };
    let public_key = PublicKey::from_openssh(material.public_line())?;
    Verifier::verify(&public_key, DATA, &signature)?;

    agent.stop().await;
    Ok(())
}

#[tokio::test]
async fn test_agent_server_skips_unparsable_key_and_keeps_serving() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (storage, identity, vault_id) = seeded_storage(dir.path()).await?;
    let material = SshKeyMaterial::keygen_ed25519("good@example.com")?;
    let recipients = vec![identity.to_recipient()];
    let mut broken = ssh_entry("broken", vault_id, &material, true);
    for field in broken.fields.iter_mut() {
        if field.field_type == FieldType::SshPrivateKey {
            field.value = FieldValue::Hidden("not a key".to_string());
        }
    }
    storage.save_entry(&broken, &recipients).await?;
    storage
        .save_entry(&ssh_entry("good", vault_id, &material, true), &recipients)
        .await?;
    let mut ring = KeyRing::new();
    let loaded = ring.populate(&storage, &identity).await?;
    let agent = RunningAgent::start(
        Arc::new(Mutex::new(ring)),
        dir.path().join("data").join("agent.sock"),
    )
    .await?;
    let mut client = agent.client().await?;

    // -- Exec
    let response = client.handle(Request::RequestIdentities).await?;

    // -- Check
    assert_eq!(loaded, 1);
    let Response::IdentitiesAnswer(identities) = response else {
        return Err(format!("expected identities answer, got {response:?}").into());
    };
    assert_eq!(identities.len(), 1);

    agent.stop().await;
    Ok(())
}

// endregion: --- Round trip

// region:    --- Refusals

#[tokio::test]
async fn test_agent_server_locked_ring_answers_failure() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("locked@example.com")?;
    let agent =
        RunningAgent::start(locked_ring(), dir.path().join("data").join("agent.sock")).await?;
    let mut client = agent.client().await?;

    // -- Exec
    let listed = client.handle(Request::RequestIdentities).await?;
    let signed = client
        .handle(Request::SignRequest(SignRequest {
            credential: PublicCredential::Key(key_data(&material)?),
            data: DATA.to_vec(),
            flags: 0,
        }))
        .await?;

    // -- Check
    assert_eq!(listed, Response::Failure);
    assert_eq!(signed, Response::Failure);

    agent.stop().await;
    Ok(())
}

#[tokio::test]
async fn test_agent_server_refuses_non_v1_messages() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (storage, identity, vault_id) = seeded_storage(dir.path()).await?;
    let material = SshKeyMaterial::keygen_ed25519("refused@example.com")?;
    storage
        .save_entry(
            &ssh_entry("github", vault_id, &material, true),
            &[identity.to_recipient()],
        )
        .await?;
    let mut ring = KeyRing::new();
    ring.populate(&storage, &identity).await?;
    let agent = RunningAgent::start(
        Arc::new(Mutex::new(ring)),
        dir.path().join("data").join("agent.sock"),
    )
    .await?;
    let mut client = agent.client().await?;

    // -- Exec
    let locked = client.handle(Request::Lock("secret".to_string())).await?;
    let unlocked = client.handle(Request::Unlock("secret".to_string())).await?;
    let removed = client.handle(Request::RemoveAllIdentities).await?;
    let extended = client
        .handle(Request::Extension(extension("bogita-keys@bogita")))
        .await?;
    let after = client.handle(Request::RequestIdentities).await?;

    // -- Check
    assert_eq!(locked, Response::Failure);
    assert_eq!(unlocked, Response::Failure);
    assert_eq!(removed, Response::Failure);
    assert_eq!(extended, Response::ExtensionFailure);
    let Response::IdentitiesAnswer(identities) = after else {
        return Err(format!("expected identities answer, got {after:?}").into());
    };
    assert_eq!(
        identities.len(),
        1,
        "refused messages must not touch entries"
    );

    agent.stop().await;
    Ok(())
}

#[tokio::test]
async fn test_agent_server_unknown_key_answers_failure() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (storage, identity, vault_id) = seeded_storage(dir.path()).await?;
    let material = SshKeyMaterial::keygen_ed25519("listed@example.com")?;
    let unknown = SshKeyMaterial::keygen_ed25519("unknown@example.com")?;
    storage
        .save_entry(
            &ssh_entry("github", vault_id, &material, true),
            &[identity.to_recipient()],
        )
        .await?;
    let mut ring = KeyRing::new();
    ring.populate(&storage, &identity).await?;
    let agent = RunningAgent::start(
        Arc::new(Mutex::new(ring)),
        dir.path().join("data").join("agent.sock"),
    )
    .await?;
    let mut client = agent.client().await?;

    // -- Exec
    let response = client
        .handle(Request::SignRequest(SignRequest {
            credential: PublicCredential::Key(key_data(&unknown)?),
            data: DATA.to_vec(),
            flags: 0,
        }))
        .await?;

    // -- Check
    assert_eq!(response, Response::Failure);

    agent.stop().await;
    Ok(())
}

// endregion: --- Refusals

// region:    --- Daemon connection

#[tokio::test]
async fn test_agent_server_daemon_connection_uses_wal_and_busy_timeout() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let storage = SqliteStorage::new_daemon(dir.path().join("vault.db"), AgeCrypto).await?;

    // -- Exec
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&storage.pool)
        .await?;
    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&storage.pool)
        .await?;

    // -- Check
    assert_eq!(journal_mode.to_lowercase(), "wal");
    assert_eq!(busy_timeout, 5000);
    Ok(())
}

// endregion: --- Daemon connection
