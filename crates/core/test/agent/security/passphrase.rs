//! The passphrase sentinel (agent/unlock.rs, agent/server/session.rs)
//!
//! The unlock extension carries the vault passphrase on the wire. The daemon must
//! derive the identity, rebuild the ring, and drop both without writing the
//! passphrase, the age identity, or the served key to stderr.
//!
//! The capture mechanics live in `support.rs`; the assertion is the cycle: a
//! refused passphrase, an accepted one, then a lock.

use super::security_support::{
    identity_middle_chunk, locked_ring, pem_middle_chunk, ssh_entry, temp_socket, FixedSource,
    Result, RunningAgent, StderrCapture, CAPTURE_MARKER,
};

use crate::agent::client;
use crate::agent::error::SshAgentError;
use crate::agent::unlock::VaultLock;
use crate::domain::ssh::SshKeyMaterial;
use crate::domain::{AgeIdentity, SecretString};
use crate::ports::EncryptedIdentityFile;
use crate::storage::identity::write_identity_encrypted;
use secrecy::ExposeSecret;
use std::sync::Arc;

/// The passphrase that travels on the accepted unlock request.
const SENTINEL_PASSPHRASE: &str = "s5-sentinel-passphrase-do-not-log";
/// The passphrase that travels on the refused unlock request.
const SENTINEL_REFUSED: &str = "s5-refused-passphrase-do-not-log";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn test_agent_security_logs_neither_passphrase_nor_identity() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(dir.path());
    let material = SshKeyMaterial::keygen_ed25519("sentinel@example.com")?;
    let pem_chunk = pem_middle_chunk(material.private_pem());
    let identity = AgeIdentity::generate();
    let secret = identity.to_secret_string();
    assert!(
        secret.expose_secret().starts_with("AGE-SECRET-KEY-"),
        "the identity must carry the age bech32 prefix the assertions pin"
    );
    let identity_chunk = identity_middle_chunk(secret.expose_secret());
    assert_eq!(
        identity_chunk.len(),
        40,
        "the identity chunk must be distinctive"
    );
    let passphrase = SecretString::from(SENTINEL_PASSPHRASE);
    let identity_path = dir.path().join("identity.age");
    write_identity_encrypted(&identity, &passphrase, &identity_path)?;

    let ring = locked_ring();
    let lock = Arc::new(VaultLock::new(
        identity_path,
        EncryptedIdentityFile,
        FixedSource::new(vec![ssh_entry("github", &material)]),
        Arc::clone(&ring),
    ));

    let capture = StderrCapture::start()?;
    capture.write_marker(CAPTURE_MARKER)?;
    let agent = RunningAgent::start_with_lock(Arc::clone(&ring), socket_path.clone(), lock).await?;

    // -- Exec: a refused passphrase, the accepted one, then a lock
    let refused = client::unlock(&socket_path, &SecretString::from(SENTINEL_REFUSED));
    let unlocked = client::unlock(&socket_path, &passphrase);
    let served = client::status(&socket_path)?;
    client::lock(&socket_path)?;
    let locked = client::status(&socket_path)?;
    agent.stop().await;

    // -- Check: the cycle really ran, both on the refusal and on the accepted path
    assert!(
        matches!(refused, Err(SshAgentError::UnlockRefused)),
        "the refused passphrase must reach the refusal path, got {refused:?}"
    );
    unlocked?;
    assert_eq!(served.as_deref(), Some("1 keys, unlocked"));
    assert_eq!(locked.as_deref(), Some("0 keys, locked"));

    // -- Check: the capture itself works
    let log = capture.finish()?;
    assert!(
        log.contains(CAPTURE_MARKER),
        "the stderr capture must observe writes, captured:\n{log}"
    );

    // -- Check: neither passphrase survives the unlock
    assert!(
        !log.contains(SENTINEL_PASSPHRASE),
        "stderr must not carry the accepted passphrase: {log}"
    );
    assert!(
        !log.contains(SENTINEL_REFUSED),
        "stderr must not carry the refused passphrase: {log}"
    );

    // -- Check: no identity material and no staged key
    assert!(
        !log.contains(&identity_chunk),
        "stderr must not carry age identity material: {log}"
    );
    assert!(
        !log.contains("AGE-SECRET-KEY"),
        "stderr must not carry the identity armor prefix: {log}"
    );
    assert!(
        !log.contains(&pem_chunk),
        "stderr must not carry private key body: {log}"
    );
    assert!(
        !log.contains("OPENSSH PRIVATE KEY"),
        "stderr must not carry the key armor header: {log}"
    );
    Ok(())
}
