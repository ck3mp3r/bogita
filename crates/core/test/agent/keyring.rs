//! Tests for the agent key ring (agent/keyring.rs)
//!
//! Pure: no database, no socket, no keychain. Keys are generated in-process.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::error::SshAgentError;
use crate::agent::keyring::{algorithm_supported, KeyRing};
use crate::domain::ssh::{agent_servable_field, SshKeyMaterial};
use crate::domain::{Entry, FieldType, FieldValue, AGENT_SERVABLE_KEY};
use crate::test_support::test_entry;
use signature::Verifier;
use ssh_key::public::KeyData;
use ssh_key::{Algorithm, PublicKey};
use uuid::Uuid;

/// Ed25519 key exported by `ssh-keygen -N <passphrase>`: aes256-ctr + bcrypt.
const PASSPHRASE_PEM: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
b3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHIAAAAGYmNyeXB0AAAAGAAAABA6VToCmz\n\
0EB/owAwgRxJIuAAAAIAAAAAEAAAAzAAAAC3NzaC1lZDI1NTE5AAAAIOnjCpt92o89fIky\n\
7G3G98I0qyBUoOxuk7uz0pqiB3LFAAAAoHJHnmtEeSydut6OGu5DSuudMGPATF7hhTNH2u\n\
wcDDWpGYmUh2HjmtOPLaQ8tWIg/e8HfmpK4CXWp8RGPk4P/Es4nkYP9/vwcwHiq1nJivym\n\
nQSnnZV81YV7TwplSBRyXURAHG8gIx5n5cIqP1EiZdo8a31+t9vRnKNBhZCwGPAyFVfHka\n\
fZ/A1kfKl44mmGXUmiWCLWvbxdrlnFzzbJy8U=\n\
-----END OPENSSH PRIVATE KEY-----\n";

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

fn entry_with_private_pem(name: &str, material: &SshKeyMaterial, pem: &str) -> Entry {
    let mut entry = ssh_entry(name, material, true);
    for field in entry.fields.iter_mut() {
        if field.field_type == FieldType::SshPrivateKey {
            field.value = FieldValue::Hidden(pem.to_string());
        }
    }
    entry
}

// endregion: --- Fixtures

// region:    --- Cache state

#[test]
fn test_agent_keyring_new_is_locked() {
    // -- Exec
    let ring = KeyRing::new();

    // -- Check
    assert!(ring.is_locked());
    assert_eq!(ring.len(), 0);
    assert!(ring.is_empty());
    assert!(ring.entries().is_empty());
}

#[test]
fn test_agent_keyring_load_entries_caches_servable_key() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("alice@example.com")?;
    let entry = ssh_entry("github", &material, true);
    let mut ring = KeyRing::new();

    // -- Exec
    let loaded = ring.load_entries(vec![entry]);

    // -- Check
    assert_eq!(loaded, 1);
    assert!(!ring.is_locked());
    assert_eq!(ring.len(), 1);
    assert_eq!(ring.entries()[0].name(), "github");
    assert_eq!(ring.entries()[0].comment(), "alice@example.com");
    assert_eq!(ring.entries()[0].public_key(), &key_data(&material)?);
    Ok(())
}

#[test]
fn test_agent_keyring_load_entries_skips_unservable_entry() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("bob@example.com")?;
    let mut ring = KeyRing::new();

    // -- Exec
    let loaded = ring.load_entries(vec![ssh_entry("github", &material, false)]);

    // -- Check
    assert_eq!(loaded, 0);
    assert!(ring.is_empty());
    assert!(!ring.is_locked(), "an unlocked vault stays unlocked");
    Ok(())
}

#[test]
fn test_agent_keyring_load_entries_skips_entry_without_ssh_key() -> Result<()> {
    // -- Setup & Fixtures
    let mut ring = KeyRing::new();
    let password_entry = crate::test_support::test_password_entry(Uuid::new_v4(), "plain");

    // -- Exec
    let loaded = ring.load_entries(vec![password_entry]);

    // -- Check
    assert_eq!(loaded, 0);
    Ok(())
}

#[test]
fn test_agent_keyring_load_entries_replaces_previous_cache() -> Result<()> {
    // -- Setup & Fixtures
    let first = SshKeyMaterial::keygen_ed25519("first@example.com")?;
    let second = SshKeyMaterial::keygen_ed25519("second@example.com")?;
    let mut ring = KeyRing::new();
    ring.load_entries(vec![ssh_entry("first", &first, true)]);

    // -- Exec
    let loaded = ring.load_entries(vec![ssh_entry("second", &second, true)]);

    // -- Check
    assert_eq!(loaded, 1);
    assert_eq!(ring.entries()[0].name(), "second");
    Ok(())
}

// endregion: --- Cache state

// region:    --- Skipped keys (criterion 7)

#[test]
fn test_agent_keyring_load_entries_skips_unparsable_pem() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("carol@example.com")?;
    let entry = entry_with_private_pem("broken", &material, "not a key");
    let mut ring = KeyRing::new();

    // -- Exec
    let loaded = ring.load_entries(vec![entry]);

    // -- Check
    assert_eq!(loaded, 0, "the unparsable key is skipped, not fatal");
    assert!(!ring.is_locked());
    Ok(())
}

#[test]
fn test_agent_keyring_load_entries_skips_passphrase_protected_pem() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("dave@example.com")?;
    let entry = entry_with_private_pem("protected", &material, PASSPHRASE_PEM);
    let mut ring = KeyRing::new();

    // -- Exec
    let loaded = ring.load_entries(vec![entry]);

    // -- Check
    assert_eq!(
        loaded, 0,
        "an encrypted PEM cannot sign without a passphrase"
    );
    Ok(())
}

#[test]
fn test_agent_keyring_load_entries_skips_text_true_gate() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("text-gate@example.com")?;
    let mut entry = ssh_entry("mangled", &material, true);
    for field in entry.fields.iter_mut() {
        if field.key == AGENT_SERVABLE_KEY {
            field.value = FieldValue::Text("true".to_string());
        }
    }
    let gate = entry
        .fields
        .iter()
        .find(|field| field.key == AGENT_SERVABLE_KEY)
        .ok_or("the fixture should carry the servable gate")?;
    assert_eq!(
        gate.field_type,
        FieldType::Boolean,
        "the mangle keeps the type and changes only the value"
    );
    let mut ring = KeyRing::new();

    // -- Exec
    let loaded = ring.load_entries(vec![entry]);

    // -- Check
    assert_eq!(
        loaded, 0,
        "the gate matches FieldValue::Boolean(true) only, never a Text true"
    );
    assert!(ring.is_empty());
    Ok(())
}

#[test]
fn test_agent_keyring_load_entries_serves_legacy_custom_typed_gate() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("legacy@example.com")?;
    let mut entry = ssh_entry("legacy", &material, true);
    for field in entry.fields.iter_mut() {
        if field.key == AGENT_SERVABLE_KEY {
            field.field_type = FieldType::Custom(AGENT_SERVABLE_KEY.to_string());
        }
    }
    let mut ring = KeyRing::new();

    // -- Exec
    let loaded = ring.load_entries(vec![entry]);

    // -- Check
    assert_eq!(loaded, 1, "the gate reads the legacy custom type");
    assert_eq!(ring.entries()[0].name(), "legacy");
    Ok(())
}

#[test]
fn test_agent_keyring_load_entries_keeps_serving_after_a_skipped_key() -> Result<()> {
    // -- Setup & Fixtures
    let good = SshKeyMaterial::keygen_ed25519("good@example.com")?;
    let broken = entry_with_private_pem("broken", &good, "");
    let mut ring = KeyRing::new();

    // -- Exec
    let loaded = ring.load_entries(vec![broken, ssh_entry("good", &good, true)]);

    // -- Check
    assert_eq!(loaded, 1);
    assert_eq!(ring.entries()[0].name(), "good");
    Ok(())
}

#[test]
fn test_agent_keyring_load_entries_reports_the_unsupported_algorithm() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::validate(crate::test_support::ECDSA_OPENSSH_PEM)?;
    let entry = ssh_entry("ecdsa-key", &material, true);
    let mut ring = KeyRing::new();

    // -- Exec
    let loaded = ring.load_entries(vec![entry]);

    // -- Check
    assert_eq!(loaded, 0, "ECDSA is outside the serve matrix");
    assert!(
        material.key_type().starts_with("ecdsa"),
        "the fixture PEM parses as ECDSA"
    );
    let reason = KeyRing::unsupported_algorithm_reason(Algorithm::Ecdsa {
        curve: ssh_key::EcdsaCurve::NistP256,
    });
    assert!(
        reason.contains("ecdsa"),
        "the warning names the algorithm: {reason}"
    );
    assert!(
        reason.contains("support matrix"),
        "the warning names the policy: {reason}"
    );
    Ok(())
}

#[test]
fn test_agent_keyring_comment_falls_back_to_pem_comment() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("erin@example.com")?;
    let mut entry = ssh_entry("no-comment-field", &material, true);
    entry
        .fields
        .retain(|f| f.field_type != FieldType::SshComment);
    let mut ring = KeyRing::new();

    // -- Exec
    ring.load_entries(vec![entry]);

    // -- Check
    assert_eq!(ring.entries()[0].comment(), "erin@example.com");
    Ok(())
}

// endregion: --- Skipped keys

// region:    --- Clear

#[test]
fn test_agent_keyring_clear_drops_cached_keys() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("frank@example.com")?;
    let mut ring = KeyRing::new();
    ring.load_entries(vec![ssh_entry("github", &material, true)]);

    // -- Exec
    ring.clear();

    // -- Check
    assert!(ring.is_locked());
    assert_eq!(ring.len(), 0);
    assert!(ring.entries().is_empty());
    Ok(())
}

// endregion: --- Clear

// region:    --- Signing

#[test]
fn test_agent_keyring_sign_produces_verifiable_ed25519_signature() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("gina@example.com")?;
    let data = b"session-bind-challenge";
    let mut ring = KeyRing::new();
    ring.load_entries(vec![ssh_entry("github", &material, true)]);

    // -- Exec
    let signature = ring.sign(&key_data(&material)?, data, None)?;

    // -- Check
    assert_eq!(signature.algorithm(), ssh_key::Algorithm::Ed25519);
    let public_key = PublicKey::from_openssh(material.public_line())?;
    Verifier::verify(&public_key, data, &signature)?;
    Ok(())
}

#[test]
fn test_agent_keyring_sign_when_locked_is_locked_error() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("hank@example.com")?;
    let ring = KeyRing::new();

    // -- Exec
    let result = ring.sign(&key_data(&material)?, b"data", None);

    // -- Check
    assert!(matches!(result, Err(SshAgentError::Locked)));
    Ok(())
}

#[test]
fn test_agent_keyring_sign_unknown_key_is_rejected() -> Result<()> {
    // -- Setup & Fixtures
    let cached = SshKeyMaterial::keygen_ed25519("cached@example.com")?;
    let unknown = SshKeyMaterial::keygen_ed25519("unknown@example.com")?;
    let mut ring = KeyRing::new();
    ring.load_entries(vec![ssh_entry("github", &cached, true)]);

    // -- Exec
    let result = ring.sign(&key_data(&unknown)?, b"data", None);

    // -- Check
    assert!(matches!(result, Err(SshAgentError::UnknownKey)));
    Ok(())
}

// endregion: --- Signing

// region:    --- Lock policy (criterion 5, 8)

#[test]
fn test_agent_keyring_algorithm_supported_matrix_is_ed25519_and_rsa() {
    // -- Check
    assert!(algorithm_supported(Algorithm::Ed25519));
    assert!(
        algorithm_supported(Algorithm::Rsa { hash: None }),
        "RSA is served through the explicit rsa-sha2 sign arm"
    );
    assert!(
        !algorithm_supported(Algorithm::Dsa),
        "ssh-key is built without dsa: no signing arm exists"
    );
    assert!(
        !algorithm_supported(Algorithm::Ecdsa {
            curve: ssh_key::EcdsaCurve::NistP256
        }),
        "ECDSA serving stays on the roadmap"
    );
}

#[test]
fn test_agent_keyring_load_entries_stamps_the_activity_clock() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("clock@example.com")?;
    let mut ring = KeyRing::new();
    assert!(ring.last_served_request().is_none());

    // -- Exec
    ring.load_entries(vec![ssh_entry("github", &material, true)]);

    // -- Check
    assert!(
        ring.last_served_request().is_some(),
        "loading the cache counts as activity"
    );
    Ok(())
}

#[test]
fn test_agent_keyring_clear_forgets_the_activity_clock() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("clock@example.com")?;
    let mut ring = KeyRing::new();
    ring.load_entries(vec![ssh_entry("github", &material, true)]);

    // -- Exec
    ring.clear();

    // -- Check
    assert!(ring.last_served_request().is_none());
    Ok(())
}

#[test]
fn test_agent_keyring_rebuild_is_refused_until_allowed() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("rebuild@example.com")?;
    let entry = ssh_entry("github", &material, true);
    let mut ring = KeyRing::new();
    assert!(!ring.rebuild_allowed(), "a fresh ring refuses rebuilds");

    // -- Exec
    let refused = ring.rebuild(vec![entry.clone()]);
    ring.allow_rebuild();
    let allowed = ring.rebuild(vec![entry]);

    // -- Check
    assert!(!refused, "a forbidden rebuild leaves the ring locked");
    assert!(allowed);
    assert_eq!(ring.len(), 1);
    Ok(())
}

#[test]
fn test_agent_keyring_forbid_rebuild_blocks_the_next_rebuild() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("evicted@example.com")?;
    let entry = ssh_entry("github", &material, true);
    let mut ring = KeyRing::new();
    ring.allow_rebuild();
    ring.rebuild(vec![entry.clone()]);

    // -- Exec
    ring.clear();
    ring.forbid_rebuild();
    let rebuilt = ring.rebuild(vec![entry]);

    // -- Check
    assert!(!rebuilt);
    assert!(ring.is_locked());
    Ok(())
}

// endregion: --- Lock policy
