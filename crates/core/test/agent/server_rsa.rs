//! Integration tests for RSA serving (agent/server.rs, agent/keyring.rs)
//!
//! Gated on the `integration` feature: these tests bind real unix sockets and
//! open real SQLite databases. The key is the committed RSA fixture PEM.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::error::SshAgentError;
use crate::agent::keyring::KeyRing;
use crate::agent::server::{serve, SharedKeyRing};
use crate::crypto::age::AgeCrypto;
use crate::domain::ssh::{agent_servable_field, SshKeyMaterial};
use crate::domain::{AgeIdentity, Entry};
use crate::ports::EntryStore;
use crate::storage::sqlite::SqliteStorage;
use crate::test_support::{seed_vault, test_entry, RSA_OPENSSH_PEM};
use signature::Verifier;
use ssh_agent_lib::agent::Session;
use ssh_agent_lib::client::Client;
use ssh_agent_lib::proto::{PublicCredential, Request, Response, SignRequest};
use ssh_key::public::{KeyData, RsaPublicKey};
use ssh_key::{Algorithm, HashAlg, PublicKey, Signature};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use uuid::Uuid;

const DATA: &[u8] = b"rsa-agent-wire-data";
const STARTUP_LIMIT: usize = 200;

/// RFC 9987 §8.3: the client asks for rsa-sha2-256 or rsa-sha2-512.
const RSA_SHA2_256_FLAG: u32 = 0x0000_0002;
const RSA_SHA2_512_FLAG: u32 = 0x0000_0004;

// region:    --- Fixtures

fn rsa_material() -> Result<SshKeyMaterial> {
    Ok(SshKeyMaterial::validate(RSA_OPENSSH_PEM)?)
}

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

fn rsa_public_key(material: &SshKeyMaterial) -> Result<RsaPublicKey> {
    match key_data(material)? {
        KeyData::Rsa(public_key) => Ok(public_key),
        other => Err(format!("fixture is not an RSA key: {other:?}").into()),
    }
}

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

/// Store the RSA fixture entry, populate the ring, and start the daemon.
async fn rsa_agent(dir: &Path) -> Result<(SshKeyMaterial, RunningAgent)> {
    let storage = SqliteStorage::new(dir.join("vault.db"), AgeCrypto).await?;
    let identity = AgeIdentity::generate();
    let vault_id = Uuid::new_v4();
    seed_vault(&storage, vault_id).await?;
    let material = rsa_material()?;
    storage
        .save_entry(
            &ssh_entry("rsa-key", vault_id, &material),
            &[identity.to_recipient()],
        )
        .await?;
    let mut ring = KeyRing::new();
    let loaded = ring.populate(&storage, &identity).await?;
    assert_eq!(loaded, 1, "an opted-in RSA key is cached, not skipped");
    let agent = RunningAgent::start(
        Arc::new(Mutex::new(ring)),
        dir.join("data").join("agent.sock"),
    )
    .await?;
    Ok((material, agent))
}

/// Send one SIGN_REQUEST and return the signature the daemon replied with.
async fn sign(
    client: &mut Client<UnixStream>,
    material: &SshKeyMaterial,
    flags: u32,
) -> Result<Signature> {
    let response = client
        .handle(Request::SignRequest(SignRequest {
            credential: PublicCredential::Key(key_data(material)?),
            data: DATA.to_vec(),
            flags,
        }))
        .await?;
    match response {
        Response::SignResponse(signature) => Ok(signature),
        other => Err(format!("expected sign response, got {other:?}").into()),
    }
}

// endregion: --- Fixtures

// region:    --- Identities

#[tokio::test]
async fn test_agent_server_lists_the_rsa_public_blob() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (material, agent) = rsa_agent(dir.path()).await?;
    let mut client = agent.client().await?;

    // -- Exec
    let response = client.handle(Request::RequestIdentities).await?;

    // -- Check
    let Response::IdentitiesAnswer(identities) = response else {
        return Err(format!("expected identities answer, got {response:?}").into());
    };
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].comment, "rsa-fixture@example.com");
    let KeyData::Rsa(listed) = identities[0].credential.key_data() else {
        return Err("the listed credential is not an RSA public key".into());
    };
    assert_eq!(listed, &rsa_public_key(&material)?);
    assert!(
        material.public_line().starts_with("ssh-rsa "),
        "the public blob keeps the ssh-rsa key type"
    );

    agent.stop().await;
    Ok(())
}

// endregion: --- Identities

// region:    --- Signing

/// Assert the reply names `hash` and verifies under the served RSA key.
fn assert_rsa_signature(
    signature: &Signature,
    material: &SshKeyMaterial,
    hash: HashAlg,
) -> Result<()> {
    assert_eq!(
        signature.algorithm(),
        Algorithm::Rsa { hash: Some(hash) },
        "SIGN_RESPONSE carries the rsa-sha2 algorithm the flags asked for"
    );
    let public_key = rsa_public_key(material)?;
    Verifier::verify(&public_key, DATA, signature)?;
    Ok(())
}

#[tokio::test]
async fn test_agent_server_signs_rsa_sha2_256_for_flag_0x2() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (material, agent) = rsa_agent(dir.path()).await?;
    let mut client = agent.client().await?;

    // -- Exec
    let signature = sign(&mut client, &material, RSA_SHA2_256_FLAG).await?;

    // -- Check
    assert_rsa_signature(&signature, &material, HashAlg::Sha256)?;

    agent.stop().await;
    Ok(())
}

#[tokio::test]
async fn test_agent_server_signs_rsa_sha2_512_for_flag_0x4() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (material, agent) = rsa_agent(dir.path()).await?;
    let mut client = agent.client().await?;

    // -- Exec
    let signature = sign(&mut client, &material, RSA_SHA2_512_FLAG).await?;

    // -- Check
    assert_rsa_signature(&signature, &material, HashAlg::Sha512)?;

    agent.stop().await;
    Ok(())
}

#[tokio::test]
async fn test_agent_server_signs_rsa_sha2_256_without_flags() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (material, agent) = rsa_agent(dir.path()).await?;
    let mut client = agent.client().await?;

    // -- Exec
    let signature = sign(&mut client, &material, 0).await?;

    // -- Check
    assert_rsa_signature(&signature, &material, HashAlg::Sha256)?;

    agent.stop().await;
    Ok(())
}

// endregion: --- Signing
