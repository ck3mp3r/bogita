//! Tests for SSH key material (domain/ssh.rs)

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::domain::ssh::{agent_servable_field, SshKeyMaterial, AGENT_SERVABLE_KEY};
use crate::domain::{Field, FieldType, FieldValue};
use ssh_key::{HashAlg, PublicKey};
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

// ── keygen ────────────────────────────────────────────────────────────────

#[test]
fn test_domain_ssh_keygen_produces_openssh_pem() -> Result<()> {
    // -- Exec
    let material = SshKeyMaterial::keygen_ed25519("alice@example.com")?;

    // -- Check
    assert!(material
        .private_pem()
        .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----\n"));
    assert!(material
        .private_pem()
        .ends_with("-----END OPENSSH PRIVATE KEY-----\n"));
    assert!(material.public_line().starts_with("ssh-ed25519 "));
    assert!(material.public_line().ends_with("alice@example.com"));
    assert_eq!(material.key_type(), "Ed25519");
    assert_eq!(material.comment(), "alice@example.com");
    assert!(material.fingerprint().starts_with("SHA256:"));
    Ok(())
}

#[test]
fn test_domain_ssh_keygen_without_comment() -> Result<()> {
    // -- Exec
    let material = SshKeyMaterial::keygen_ed25519("")?;

    // -- Check
    assert_eq!(material.comment(), "");
    assert!(material.public_line().starts_with("ssh-ed25519 "));
    Ok(())
}

#[test]
fn test_domain_ssh_keygen_generates_distinct_keys() -> Result<()> {
    // -- Exec
    let first = SshKeyMaterial::keygen_ed25519("one")?;
    let second = SshKeyMaterial::keygen_ed25519("one")?;

    // -- Check
    assert_ne!(first.private_pem(), second.private_pem());
    assert_ne!(first.public_line(), second.public_line());
    Ok(())
}

// ── validate ──────────────────────────────────────────────────────────────

#[test]
fn test_domain_ssh_validate_roundtrips_generated_key() -> Result<()> {
    // -- Setup & Fixtures
    let generated = SshKeyMaterial::keygen_ed25519("bob@example.com")?;

    // -- Exec
    let validated = SshKeyMaterial::validate(generated.private_pem())?;

    // -- Check
    assert_eq!(validated.public_line(), generated.public_line());
    assert_eq!(validated.key_type(), "Ed25519");
    assert_eq!(validated.comment(), "bob@example.com");
    assert_eq!(validated.fingerprint(), generated.fingerprint());
    Ok(())
}

#[test]
fn test_domain_ssh_fingerprint_matches_public_line() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("carol@example.com")?;

    // -- Exec
    let parsed = PublicKey::from_openssh(material.public_line())?;
    let expected = parsed.fingerprint(HashAlg::Sha256).to_string();

    // -- Check
    assert_eq!(material.fingerprint(), expected);
    Ok(())
}

#[test]
fn test_domain_ssh_validate_rejects_non_pem() -> Result<()> {
    // -- Exec
    let result = SshKeyMaterial::validate("definitely not a key");

    // -- Check
    let message = match result {
        Err(e) => e.to_string(),
        Ok(_) => return Err("non-PEM input must be rejected".into()),
    };
    assert!(
        message.contains("invalid openssh pem"),
        "unexpected message: {message}"
    );
    Ok(())
}

#[test]
fn test_domain_ssh_validate_rejects_empty_input() -> Result<()> {
    // -- Exec
    let result = SshKeyMaterial::validate("");

    // -- Check
    assert!(result.is_err());
    Ok(())
}

#[test]
fn test_domain_ssh_validate_rejects_passphrase_protected_pem() -> Result<()> {
    // -- Exec
    let result = SshKeyMaterial::validate(PASSPHRASE_PEM);

    // -- Check
    let message = match result {
        Err(e) => e.to_string(),
        Ok(_) => return Err("passphrase-protected PEM must be rejected".into()),
    };
    assert!(
        message.contains("passphrase-protected keys are not supported in v1"),
        "unexpected message: {message}"
    );
    Ok(())
}

// ── field mapping ─────────────────────────────────────────────────────────

fn field_type_values(fields: &[Field]) -> Vec<(FieldType, String)> {
    fields
        .iter()
        .map(|f| (f.field_type.clone(), f.value.to_string()))
        .collect()
}

#[test]
fn test_domain_ssh_to_fields_builds_four_ssh_fields() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("dave@example.com")?;

    // -- Exec
    let fields = material.to_fields();

    // -- Check
    assert_eq!(fields.len(), 4);
    assert_eq!(
        fields
            .iter()
            .map(|f| f.field_type.clone())
            .collect::<Vec<_>>(),
        vec![
            FieldType::SshPrivateKey,
            FieldType::SshPublicKey,
            FieldType::SshKeyType,
            FieldType::SshComment,
        ]
    );
    assert!(fields[0].encrypted, "private key field is encrypted");
    assert!(matches!(fields[0].value, FieldValue::Hidden(_)));
    assert!(!fields[1].encrypted);
    assert_eq!(fields[1].value.to_string(), material.public_line());
    assert_eq!(fields[2].value.to_string(), "Ed25519");
    assert_eq!(fields[3].value.to_string(), "dave@example.com");
    assert_eq!(
        fields.iter().map(|f| f.idx).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    Ok(())
}

#[test]
fn test_domain_ssh_apply_to_replaces_ssh_values_and_keeps_ids() -> Result<()> {
    // -- Setup & Fixtures
    let retired = SshKeyMaterial::keygen_ed25519("erin@example.com")?;
    let fresh = SshKeyMaterial::keygen_ed25519("erin@example.com")?;
    let mut fields = retired.to_fields();
    fields.push(agent_servable_field(true, 4));
    let original_ids: Vec<Uuid> = fields.iter().map(|f| f.id).collect();

    // -- Exec
    fresh.apply_to(&mut fields);

    // -- Check
    assert_eq!(
        fields.iter().map(|f| f.id).collect::<Vec<_>>(),
        original_ids
    );
    let values = field_type_values(&fields);
    assert!(values.contains(&(FieldType::SshPrivateKey, fresh.private_pem().to_string())));
    assert!(values.contains(&(FieldType::SshPublicKey, fresh.public_line().to_string())));
    assert!(!values.contains(&(FieldType::SshPrivateKey, retired.private_pem().to_string())));
    assert!(fields
        .iter()
        .any(|f| f.key == AGENT_SERVABLE_KEY && f.value == FieldValue::Boolean(true)));
    Ok(())
}

#[test]
fn test_domain_ssh_apply_to_appends_missing_fields() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("frank@example.com")?;
    let mut fields = vec![Field {
        id: Uuid::new_v4(),
        key: "ssh-key".to_string(),
        value: FieldValue::Hidden("free text from the TUI".to_string()),
        field_type: FieldType::SshPrivateKey,
        encrypted: true,
        idx: 0,
    }];

    // -- Exec
    material.apply_to(&mut fields);

    // -- Check
    assert_eq!(fields.len(), 4);
    assert_eq!(fields[0].key, "ssh-key", "existing key name is preserved");
    assert_eq!(fields[0].value.to_string(), material.private_pem());
    let types: Vec<FieldType> = fields.iter().map(|f| f.field_type.clone()).collect();
    assert!(types.contains(&FieldType::SshPublicKey));
    assert!(types.contains(&FieldType::SshKeyType));
    assert!(types.contains(&FieldType::SshComment));
    Ok(())
}

#[test]
fn test_domain_ssh_agent_servable_field_is_plaintext_boolean() -> Result<()> {
    // -- Exec
    let field = agent_servable_field(true, 4);

    // -- Check
    assert_eq!(field.key, AGENT_SERVABLE_KEY);
    assert_eq!(field.field_type, FieldType::Boolean);
    assert_eq!(field.value, FieldValue::Boolean(true));
    assert!(!field.encrypted);
    assert_eq!(field.idx, 4);
    Ok(())
}
