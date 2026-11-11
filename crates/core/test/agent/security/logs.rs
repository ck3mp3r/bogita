//! Log guards for the agent daemon (agent/keyring.rs, agent/server.rs)
//!
//! Covered here:
//! - a full list + sign cycle writes no key material, no signature bytes, and no
//!   field values to stderr
//! - a skipped key is reported by entry name only
//!
//! The passphrase sentinel lives in `passphrase.rs`.

use super::security_support::{
    hex, pem_middle_chunk, ring_with, ssh_entry, temp_socket, Result, RunningAgent, StderrCapture,
    CAPTURE_MARKER, DATA, SECRET_FIELD_VALUE,
};

use crate::domain::ssh::SshKeyMaterial;
use crate::domain::{FieldType, FieldValue};
use ssh_agent_lib::agent::Session;
use ssh_agent_lib::proto::{PublicCredential, Request, Response, SignRequest};
use ssh_key::PublicKey;

// region:    --- No key material in logs

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn test_agent_security_logs_no_key_material() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(dir.path());
    let material = SshKeyMaterial::keygen_ed25519("logged@example.com")?;
    let middle = pem_middle_chunk(material.private_pem());
    assert_eq!(middle.len(), 40, "the PEM chunk must be distinctive");

    let capture = StderrCapture::start()?;
    capture.write_marker(CAPTURE_MARKER)?;
    let ring = ring_with(vec![ssh_entry("github", &material)]);
    let agent = RunningAgent::start(ring, socket_path).await?;

    // -- Exec: a full list + sign cycle
    let mut client = agent.client().await?;
    let listed = client.handle(Request::RequestIdentities).await?;
    let signed = client
        .handle(Request::SignRequest(SignRequest {
            credential: PublicCredential::Key(
                PublicKey::from_openssh(material.public_line())?
                    .key_data()
                    .clone(),
            ),
            data: DATA.to_vec(),
            flags: 0,
        }))
        .await?;
    agent.stop().await;

    // -- Check: the cycle really happened
    assert!(matches!(listed, Response::IdentitiesAnswer(_)));
    let Response::SignResponse(signature) = signed else {
        return Err(format!("expected a sign response, got {signed:?}").into());
    };
    let signature_bytes = hex(signature.as_bytes());

    // -- Check: the capture itself works
    let log = capture.finish()?;
    assert!(
        log.contains(CAPTURE_MARKER),
        "the stderr capture must observe writes, captured:\n{log}"
    );

    // -- Check: no key material, signature bytes, or field values
    assert!(
        !log.contains(&middle),
        "stderr must not carry private key body: {log}"
    );
    assert!(
        !log.contains("OPENSSH PRIVATE KEY"),
        "stderr must not carry the key armor header: {log}"
    );
    assert!(
        !log.contains(&signature_bytes),
        "stderr must not carry signature bytes: {log}"
    );
    assert!(
        !log.contains(SECRET_FIELD_VALUE),
        "stderr must not carry field values: {log}"
    );
    assert!(
        !log.contains(material.comment()),
        "stderr must not carry the advertised comment: {log}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn test_agent_security_reports_a_skipped_key_by_entry_name_only() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(dir.path());
    let material = SshKeyMaterial::keygen_ed25519("skipped@example.com")?;
    let mut broken = ssh_entry("broken", &material);
    for field in broken.fields.iter_mut() {
        if field.field_type == FieldType::SshPrivateKey {
            field.value = FieldValue::Hidden("s5-broken-pem-value".to_string());
        }
    }
    let good = ssh_entry("github", &material);

    // -- Exec
    let capture = StderrCapture::start()?;
    let ring = ring_with(vec![broken, good]);
    let agent = RunningAgent::start(ring, socket_path).await?;
    let mut client = agent.client().await?;
    let listed = client.handle(Request::RequestIdentities).await?;
    agent.stop().await;
    let log = capture.finish()?;

    // -- Check: the warning names the entry and nothing else
    assert!(
        log.contains("entry 'broken'"),
        "a skipped key must be reported by entry name, captured:\n{log}"
    );
    assert!(
        !log.contains("s5-broken-pem-value"),
        "the skipped key material must not be logged: {log}"
    );
    let Response::IdentitiesAnswer(identities) = listed else {
        return Err(format!("expected identities answer, got {listed:?}").into());
    };
    assert_eq!(identities.len(), 1, "the parsable key must still be served");
    Ok(())
}

// endregion: --- No key material in logs
