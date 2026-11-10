//! Serve opt-in tests for SSH key entries (`handlers/entry.rs::handle_serve`).
//!
//! The support matrix is Ed25519 and RSA: an RSA key can be served, an ECDSA key
//! cannot, so opting an ECDSA key in must fail loud. This file holds every serve
//! test and splits from `entry_ssh.rs` because that file reached the 500-line
//! test limit.

use super::entry_ssh_test::{agent_servable, reload, ssh_entry_with_private_pem};
use crate::handlers::entry::{handle_keygen, handle_serve};
use crate::test_support::make_registry;
use bogita_core::domain::{
    agent_servable_field, Entry, EntryType, FieldType, FieldValue, AGENT_SERVABLE_KEY,
};
use bogita_core::test_support::{
    test_entry, test_field, AGE_ENCRYPTED_FILE, ECDSA_OPENSSH_PEM, RSA_OPENSSH_PEM,
};
use uuid::Uuid;

// Result alias kept local so it shadows the handler error type.
type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::test]
async fn serve_accepts_rsa_key_opt_in() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let fixture = ssh_entry_with_private_pem("rsa-key", RSA_OPENSSH_PEM);
    let (registry, identity, vault) = make_registry(&db, &[fixture]).await?;

    // -- Exec
    handle_serve("rsa-key".to_string(), true, None, registry, &identity).await?;

    // -- Check
    let after = reload(&db, vault.id, &identity, "rsa-key").await?;
    let field = agent_servable(&after)?;
    assert_eq!(field.value, FieldValue::Boolean(true));
    assert!(!field.encrypted, "agent_servable stays plaintext");
    Ok(())
}

#[tokio::test]
async fn serve_refuses_ecdsa_key_opt_in() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let fixture = ssh_entry_with_private_pem("ecdsa-key", ECDSA_OPENSSH_PEM);
    let (registry, identity, vault) = make_registry(&db, &[fixture]).await?;

    // -- Exec
    let result = handle_serve("ecdsa-key".to_string(), true, None, registry, &identity).await;

    // -- Check
    let message = match result {
        Err(e) => e.to_string(),
        Ok(()) => return Err("the ECDSA opt-in must be refused, but it was accepted".into()),
    };
    assert!(
        message.contains("ecdsa-key"),
        "the message names the entry: {message}"
    );
    assert!(
        message.contains("ecdsa"),
        "the message names the algorithm: {message}"
    );
    assert!(
        message.contains("not supported yet"),
        "the message says the matrix cannot serve it yet: {message}"
    );
    assert!(
        message.contains("Ed25519 and RSA"),
        "the message states the support matrix: {message}"
    );

    let after = reload(&db, vault.id, &identity, "ecdsa-key").await?;
    assert_eq!(after.fields.len(), 1, "a refused opt-in writes no field");
    Ok(())
}

#[tokio::test]
async fn serve_off_never_refuses_an_unsupported_algorithm() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let mut fixture = ssh_entry_with_private_pem("ecdsa-key", ECDSA_OPENSSH_PEM);
    fixture.fields.push(agent_servable_field(true, 1));
    let (registry, identity, vault) = make_registry(&db, &[fixture]).await?;

    // -- Exec
    handle_serve("ecdsa-key".to_string(), false, None, registry, &identity).await?;

    // -- Check
    let after = reload(&db, vault.id, &identity, "ecdsa-key").await?;
    assert_eq!(
        agent_servable(&after)?.value,
        FieldValue::Boolean(false),
        "a user can always turn serving off"
    );
    Ok(())
}

// ── unparsable and missing private keys ──────────────────────────────────

#[tokio::test]
async fn test_handle_serve_refuses_unparsable_private_key() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let fixture = ssh_entry_with_private_pem("age-blob-key", AGE_ENCRYPTED_FILE);
    let (registry, identity, vault) = make_registry(&db, &[fixture]).await?;

    // -- Exec
    let result = handle_serve("age-blob-key".to_string(), true, None, registry, &identity).await;

    // -- Check
    let message = match result {
        Err(e) => e.to_string(),
        Ok(()) => {
            return Err("an unparsable private key must be refused, but it was served".into())
        }
    };
    assert!(
        message.contains("invalid openssh pem"),
        "the message names the parse defect: {message}"
    );
    let after = reload(&db, vault.id, &identity, "age-blob-key").await?;
    assert!(
        !after.fields.iter().any(|f| f.key == AGENT_SERVABLE_KEY),
        "a refused opt-in writes no field"
    );
    Ok(())
}

#[tokio::test]
async fn test_handle_serve_refuses_entry_without_private_key_field() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let fixture = Entry {
        entry_type: EntryType::SshKey,
        ..test_entry(
            Uuid::nil(),
            "public-only",
            vec![test_field(
                "public_key",
                FieldValue::Text("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI".to_string()),
                FieldType::SshPublicKey,
                false,
                0,
            )],
        )
    };
    let (registry, identity, _vault) = make_registry(&db, &[fixture]).await?;

    // -- Exec
    let result = handle_serve("public-only".to_string(), true, None, registry, &identity).await;

    // -- Check
    let message = match result {
        Err(e) => e.to_string(),
        Ok(()) => {
            return Err("a key with no private PEM must be refused, but it was served".into())
        }
    };
    assert!(
        message.contains("no private key field to serve"),
        "the message says which field is missing: {message}"
    );
    Ok(())
}

#[tokio::test]
async fn test_handle_serve_toggle_off_allows_garbage_key() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let mut fixture = ssh_entry_with_private_pem("age-blob-key", AGE_ENCRYPTED_FILE);
    fixture.fields.push(agent_servable_field(true, 1));
    let (registry, identity, vault) = make_registry(&db, &[fixture]).await?;

    // -- Exec
    handle_serve("age-blob-key".to_string(), false, None, registry, &identity).await?;

    // -- Check
    let after = reload(&db, vault.id, &identity, "age-blob-key").await?;
    assert_eq!(
        agent_servable(&after)?.value,
        FieldValue::Boolean(false),
        "turning serving off never needs a parsable key"
    );
    Ok(())
}

#[tokio::test]
async fn serve_accepts_ed25519_key_opt_in() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let (registry, identity, vault) = make_registry(&db, &[]).await?;
    handle_keygen(
        Some("judy@host".to_string()),
        None,
        None,
        registry.clone(),
        &identity,
    )
    .await?;

    // -- Exec
    handle_serve("judy@host".to_string(), true, None, registry, &identity).await?;

    // -- Check
    let after = reload(&db, vault.id, &identity, "judy@host").await?;
    let field = agent_servable(&after)?;
    assert_eq!(field.value, FieldValue::Boolean(true));
    assert!(!field.encrypted, "agent_servable stays plaintext");
    Ok(())
}

// ── opt-in toggling ───────────────────────────────────────────────────────

#[tokio::test]
async fn serve_toggles_agent_servable_plaintext_boolean() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let (registry, identity, vault) = make_registry(&db, &[]).await?;
    handle_keygen(
        Some("heidi@host".to_string()),
        None,
        None,
        registry.clone(),
        &identity,
    )
    .await?;

    // -- Exec
    handle_serve(
        "heidi@host".to_string(),
        true,
        None,
        registry.clone(),
        &identity,
    )
    .await?;

    // -- Check
    let on = reload(&db, vault.id, &identity, "heidi@host").await?;
    let field = agent_servable(&on)?;
    assert_eq!(field.value, FieldValue::Boolean(true));
    assert!(!field.encrypted, "agent_servable stays plaintext");

    handle_serve("heidi@host".to_string(), false, None, registry, &identity).await?;

    let off = reload(&db, vault.id, &identity, "heidi@host").await?;
    let field = agent_servable(&off)?;
    assert_eq!(field.value, FieldValue::Boolean(false));
    assert!(!field.encrypted);
    assert_eq!(off.fields.len(), 5, "toggling never duplicates fields");
    Ok(())
}

#[tokio::test]
async fn serve_appends_agent_servable_when_missing() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let fixture = ssh_entry_with_private_pem("ivy", RSA_OPENSSH_PEM);
    let (registry, identity, vault) = make_registry(&db, &[fixture]).await?;

    // -- Exec
    handle_serve("ivy".to_string(), true, None, registry, &identity).await?;

    // -- Check
    let after = reload(&db, vault.id, &identity, "ivy").await?;
    assert_eq!(after.fields.len(), 2);
    assert_eq!(agent_servable(&after)?.value, FieldValue::Boolean(true));
    Ok(())
}

#[tokio::test]
async fn serve_unknown_entry_errors() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let (registry, identity, _vault) = make_registry(&db, &[]).await?;

    // -- Exec
    let result = handle_serve("nope".to_string(), true, None, registry, &identity).await;

    // -- Check
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn test_handle_serve_normalizes_legacy_field_type() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let mut fixture = ssh_entry_with_private_pem("legacy-key", RSA_OPENSSH_PEM);
    for field in fixture.fields.iter_mut() {
        if field.key == AGENT_SERVABLE_KEY {
            field.field_type = FieldType::Custom(AGENT_SERVABLE_KEY.to_string());
            field.value = FieldValue::Text("true".to_string());
        }
    }
    let (registry, identity, vault) = make_registry(&db, &[fixture]).await?;

    // -- Exec
    handle_serve("legacy-key".to_string(), true, None, registry, &identity).await?;

    // -- Check
    let after = reload(&db, vault.id, &identity, "legacy-key").await?;
    let field = agent_servable(&after)?;
    assert_eq!(
        field.field_type,
        FieldType::Boolean,
        "the heal rewrites the stored type"
    );
    assert_eq!(field.value, FieldValue::Boolean(true));
    assert!(!field.encrypted, "agent_servable stays plaintext");
    Ok(())
}
