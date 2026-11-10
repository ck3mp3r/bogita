//! Tests for the agent session handlers (agent/server.rs)
//!
//! Pure: the ring is populated from in-memory entries and the session is driven
//! directly through `Session::handle`. No database, no socket.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::keyring::KeyRing;
use crate::agent::server::{BogitaSession, SharedKeyRing};
use crate::domain::ssh::{agent_servable_field, SshKeyMaterial};
use crate::domain::Entry;
use crate::test_support::test_entry;
use signature::Verifier;
use ssh_agent_lib::agent::Session;
use ssh_agent_lib::error::AgentError;
use ssh_agent_lib::proto::{Extension, PublicCredential, Request, Response, SignRequest};
use ssh_key::public::KeyData;
use ssh_key::PublicKey;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const DATA: &[u8] = b"ssh-agent-sign-request-data";

// region:    --- Fixtures

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

fn ring_with(materials: &[(&str, &SshKeyMaterial, bool)]) -> SharedKeyRing {
    let mut ring = KeyRing::new();
    ring.load_entries(
        materials
            .iter()
            .map(|(name, material, servable)| ssh_entry(name, material, *servable))
            .collect::<Vec<_>>(),
    );
    Arc::new(Mutex::new(ring))
}

fn sign_request(material: &SshKeyMaterial) -> Result<Request> {
    Ok(Request::SignRequest(SignRequest {
        credential: PublicCredential::Key(key_data(material)?),
        data: DATA.to_vec(),
        flags: 0,
    }))
}

fn unknown_extension() -> Extension {
    Extension {
        name: "bogita-keys@bogita".to_string(),
        details: Vec::new().into(),
    }
}

fn extension(name: &str) -> Extension {
    Extension {
        name: name.to_string(),
        details: Vec::new().into(),
    }
}

// endregion: --- Fixtures

// region:    --- REQUEST_IDENTITIES

#[tokio::test]
async fn test_agent_session_request_identities_lists_servable_keys() -> Result<()> {
    // -- Setup & Fixtures
    let servable = SshKeyMaterial::keygen_ed25519("alice@example.com")?;
    let hidden = SshKeyMaterial::keygen_ed25519("hidden@example.com")?;
    let mut session = BogitaSession::new(
        ring_with(&[("github", &servable, true), ("other", &hidden, false)]),
        true,
    );

    // -- Exec
    let response = session.handle(Request::RequestIdentities).await?;

    // -- Check
    let Response::IdentitiesAnswer(identities) = response else {
        return Err(format!("expected identities answer, got {response:?}").into());
    };
    assert_eq!(identities.len(), 1, "only servable keys are listed");
    assert_eq!(identities[0].comment, "alice@example.com");
    assert_eq!(identities[0].credential.key_data(), &key_data(&servable)?);
    Ok(())
}

#[tokio::test]
async fn test_agent_session_request_identities_when_locked_fails() {
    // -- Setup & Fixtures
    let mut session = BogitaSession::new(Arc::new(Mutex::new(KeyRing::new())), true);

    // -- Exec
    let result = session.handle(Request::RequestIdentities).await;

    // -- Check
    assert!(result.is_err(), "a locked ring must not answer identities");
}

#[tokio::test]
async fn test_agent_session_request_identities_without_permission_fails() -> Result<()> {
    // -- Setup & Fixtures
    let servable = SshKeyMaterial::keygen_ed25519("alice@example.com")?;
    let mut session = BogitaSession::new(ring_with(&[("github", &servable, true)]), false);

    // -- Exec
    let result = session.handle(Request::RequestIdentities).await;

    // -- Check
    assert!(result.is_err(), "a rejected peer must not list keys");
    Ok(())
}

// endregion: --- REQUEST_IDENTITIES

// region:    --- SIGN_REQUEST

#[tokio::test]
async fn test_agent_session_sign_returns_verifiable_signature() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("bob@example.com")?;
    let mut session = BogitaSession::new(ring_with(&[("github", &material, true)]), true);

    // -- Exec
    let response = session.handle(sign_request(&material)?).await?;

    // -- Check
    let Response::SignResponse(signature) = response else {
        return Err(format!("expected sign response, got {response:?}").into());
    };
    let public_key = PublicKey::from_openssh(material.public_line())?;
    Verifier::verify(&public_key, DATA, &signature)?;
    Ok(())
}

#[tokio::test]
async fn test_agent_session_sign_unknown_key_fails() -> Result<()> {
    // -- Setup & Fixtures
    let cached = SshKeyMaterial::keygen_ed25519("cached@example.com")?;
    let unknown = SshKeyMaterial::keygen_ed25519("unknown@example.com")?;
    let mut session = BogitaSession::new(ring_with(&[("github", &cached, true)]), true);

    // -- Exec
    let result = session.handle(sign_request(&unknown)?).await;

    // -- Check
    assert!(result.is_err(), "an unlisted key must not sign");
    Ok(())
}

#[tokio::test]
async fn test_agent_session_sign_when_locked_fails() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("carol@example.com")?;
    let mut session = BogitaSession::new(Arc::new(Mutex::new(KeyRing::new())), true);

    // -- Exec
    let result = session.handle(sign_request(&material)?).await;

    // -- Check
    assert!(result.is_err(), "a locked ring must not sign");
    Ok(())
}

// endregion: --- SIGN_REQUEST

// region:    --- Control extensions

#[tokio::test]
async fn test_agent_session_status_extension_reports_locked_ring() -> Result<()> {
    // -- Setup & Fixtures
    let mut session = BogitaSession::new(Arc::new(Mutex::new(KeyRing::new())), true);

    // -- Exec
    let response = session
        .handle(Request::Extension(extension("bogita-status@bogita")))
        .await?;

    // -- Check
    let Response::ExtensionResponse(reply) = response else {
        return Err(format!("expected an extension response, got {response:?}").into());
    };
    assert_eq!(reply.name, "bogita-status@bogita");
    assert_eq!(
        std::str::from_utf8(reply.details.as_ref())?,
        "0 keys, locked"
    );
    Ok(())
}

#[tokio::test]
async fn test_agent_session_status_extension_counts_served_keys() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("status@example.com")?;
    let mut session = BogitaSession::new(ring_with(&[("github", &material, true)]), true);

    // -- Exec
    let response = session
        .handle(Request::Extension(extension("bogita-status@bogita")))
        .await?;

    // -- Check
    let Response::ExtensionResponse(reply) = response else {
        return Err(format!("expected an extension response, got {response:?}").into());
    };
    assert_eq!(
        std::str::from_utf8(reply.details.as_ref())?,
        "1 keys, unlocked"
    );
    Ok(())
}

#[tokio::test]
async fn test_agent_session_stop_extension_answers_success() -> Result<()> {
    // -- Setup & Fixtures
    let mut session = BogitaSession::new(Arc::new(Mutex::new(KeyRing::new())), true);

    // -- Exec
    let response = session
        .handle(Request::Extension(extension("bogita-stop@bogita")))
        .await?;

    // -- Check
    assert_eq!(response, Response::Success);
    Ok(())
}

#[tokio::test]
async fn test_agent_session_control_extensions_without_permission_fail() -> Result<()> {
    // -- Setup & Fixtures
    let mut session = BogitaSession::new(Arc::new(Mutex::new(KeyRing::new())), false);

    // -- Exec
    let status = session
        .handle(Request::Extension(extension("bogita-status@bogita")))
        .await;
    let stop = session
        .handle(Request::Extension(extension("bogita-stop@bogita")))
        .await;

    // -- Check
    assert!(
        matches!(status, Err(AgentError::ExtensionFailure)),
        "a rejected peer must not read the status, got {status:?}"
    );
    assert!(
        matches!(stop, Err(AgentError::ExtensionFailure)),
        "a rejected peer must not stop the daemon, got {stop:?}"
    );
    Ok(())
}

// endregion: --- Control extensions

// region:    --- Refusals

#[tokio::test]
async fn test_agent_session_extension_is_refused() {
    // -- Setup & Fixtures
    let mut session = BogitaSession::new(Arc::new(Mutex::new(KeyRing::new())), true);

    // -- Exec
    let result = session
        .handle(Request::Extension(unknown_extension()))
        .await;

    // -- Check
    assert!(
        matches!(result, Err(AgentError::ExtensionFailure)),
        "an unknown EXTENSION must answer EXTENSION_FAILURE, got {result:?}"
    );
}

#[tokio::test]
async fn test_agent_session_lock_request_is_refused() {
    // -- Setup & Fixtures
    let mut session = BogitaSession::new(Arc::new(Mutex::new(KeyRing::new())), true);

    // -- Exec
    let result = session
        .handle(Request::Lock("passphrase".to_string()))
        .await;

    // -- Check
    assert!(result.is_err(), "LOCK(22) is not a v1 message");
}

#[tokio::test]
async fn test_agent_session_remove_all_identities_is_refused() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("dave@example.com")?;
    let ring = ring_with(&[("github", &material, true)]);
    let mut session = BogitaSession::new(ring.clone(), true);

    // -- Exec
    let result = session.handle(Request::RemoveAllIdentities).await;

    // -- Check
    assert!(
        result.is_err(),
        "REMOVE_ALL_IDENTITIES(19) is not a v1 message"
    );
    assert_eq!(ring.lock().expect("ring mutex poisoned").len(), 1);
    Ok(())
}

// endregion: --- Refusals
