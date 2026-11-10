//! RSA serving tests for the agent key ring (agent/keyring.rs)
//!
//! Pure: no database, no socket, no keychain. The RSA key is the committed
//! fixture PEM; nothing is generated at runtime.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::keyring::{rsa_hash_alg, KeyRing};
use crate::domain::ssh::{agent_servable_field, SshKeyMaterial};
use crate::domain::Entry;
use crate::test_support::{test_entry, RSA_OPENSSH_PEM};
use signature::Verifier;
use ssh_key::public::{KeyData, RsaPublicKey};
use ssh_key::{Algorithm, HashAlg, PublicKey, Signature};
use uuid::Uuid;

const DATA: &[u8] = b"rsa-agent-sign-request-data";

// region:    --- Fixtures

fn rsa_material() -> Result<SshKeyMaterial> {
    Ok(SshKeyMaterial::validate(RSA_OPENSSH_PEM)?)
}

fn ssh_entry(name: &str, material: &SshKeyMaterial) -> Entry {
    let mut fields = material.to_fields();
    fields.push(agent_servable_field(true, fields.len() as i32));
    test_entry(Uuid::new_v4(), name, fields)
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

/// A ring holding the RSA fixture key.
fn rsa_ring() -> Result<(KeyRing, SshKeyMaterial)> {
    let material = rsa_material()?;
    let mut ring = KeyRing::new();
    assert_eq!(ring.load_entries(vec![ssh_entry("rsa", &material)]), 1);
    Ok((ring, material))
}

/// Assert the reply names `hash` and verifies under the fixture's RSA key.
fn assert_rsa_signature(signature: &Signature, hash: HashAlg) -> Result<()> {
    assert_eq!(
        signature.algorithm(),
        Algorithm::Rsa { hash: Some(hash) },
        "the reply names the rsa-sha2 algorithm"
    );
    let public_key = rsa_public_key(&rsa_material()?)?;
    Verifier::verify(&public_key, DATA, signature)?;
    Ok(())
}

// endregion: --- Fixtures

// region:    --- Flags

#[test]
fn test_agent_keyring_rsa_hash_alg_follows_the_sign_request_flags() {
    // -- Check
    assert_eq!(
        rsa_hash_alg(0x0000_0004),
        Some(HashAlg::Sha512),
        "SSH_AGENT_RSA_SHA2_512 asks for rsa-sha2-512"
    );
    assert_eq!(
        rsa_hash_alg(0x0000_0002),
        Some(HashAlg::Sha256),
        "SSH_AGENT_RSA_SHA2_256 asks for rsa-sha2-256"
    );
    assert_eq!(
        rsa_hash_alg(0),
        Some(HashAlg::Sha256),
        "a client that sends no flag gets the modern default"
    );
    assert_eq!(
        rsa_hash_alg(0x0000_0006),
        Some(HashAlg::Sha512),
        "a 512 request wins over a 256 bit in the same bitfield"
    );
}

// endregion: --- Flags

// region:    --- Cache

#[test]
fn test_agent_keyring_load_entries_caches_the_rsa_key() -> Result<()> {
    // -- Setup & Fixtures
    let material = rsa_material()?;
    let mut ring = KeyRing::new();

    // -- Exec
    let loaded = ring.load_entries(vec![ssh_entry("rsa-key", &material)]);

    // -- Check
    assert_eq!(loaded, 1, "RSA is inside the serve matrix");
    assert!(matches!(ring.entries()[0].public_key(), KeyData::Rsa(_)));
    assert!(
        material.public_line().starts_with("ssh-rsa "),
        "the advertised public blob keeps the ssh-rsa key type"
    );
    Ok(())
}

// endregion: --- Cache

// region:    --- Signing

#[test]
fn test_agent_keyring_sign_rsa_sha2_256_produces_verifiable_signature() -> Result<()> {
    // -- Setup & Fixtures
    let (ring, material) = rsa_ring()?;

    // -- Exec
    let signature = ring.sign(&key_data(&material)?, DATA, Some(HashAlg::Sha256))?;

    // -- Check
    assert_rsa_signature(&signature, HashAlg::Sha256)
}

#[test]
fn test_agent_keyring_sign_rsa_sha2_512_produces_verifiable_signature() -> Result<()> {
    // -- Setup & Fixtures
    let (ring, material) = rsa_ring()?;

    // -- Exec
    let signature = ring.sign(&key_data(&material)?, DATA, Some(HashAlg::Sha512))?;

    // -- Check
    assert_rsa_signature(&signature, HashAlg::Sha512)
}

#[test]
fn test_agent_keyring_sign_rsa_defaults_to_sha2_256_without_a_hash() -> Result<()> {
    // -- Setup & Fixtures
    let (ring, material) = rsa_ring()?;

    // -- Exec
    let signature = ring.sign(&key_data(&material)?, DATA, None)?;

    // -- Check
    assert_rsa_signature(&signature, HashAlg::Sha256)
}

#[test]
fn test_agent_keyring_sign_ed25519_ignores_the_rsa_hash() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("kim@example.com")?;
    let mut ring = KeyRing::new();
    ring.load_entries(vec![ssh_entry("ed25519", &material)]);

    // -- Exec
    let signature = ring.sign(&key_data(&material)?, DATA, Some(HashAlg::Sha512))?;

    // -- Check
    assert_eq!(signature.algorithm(), Algorithm::Ed25519);
    let public_key = PublicKey::from_openssh(material.public_line())?;
    Verifier::verify(&public_key, DATA, &signature)?;
    Ok(())
}

// endregion: --- Signing
