use crate::handlers::entry::{handle_keygen, handle_pubkey, handle_rotate};
use crate::test_support::make_registry;
use bogita_core::crypto::AgeCrypto;
use bogita_core::domain::{AgeIdentity, Entry, EntryType, Field, FieldType, FieldValue};
use bogita_core::ports::EntryStore;
use bogita_core::storage::sqlite::SqliteStorage;
use bogita_core::test_support::{test_entry, test_field};
use ssh_key::{HashAlg, PublicKey};
use std::path::Path;
use uuid::Uuid;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

// ── helpers ───────────────────────────────────────────────────────────────

/// Read the stored entry back through a fresh storage handle.
pub(super) async fn reload(
    db: &Path,
    vault_id: Uuid,
    identity: &AgeIdentity,
    name: &str,
) -> Result<Entry> {
    let storage = SqliteStorage::new(db, AgeCrypto).await?;
    let entries = storage.list_entries(vault_id, None, identity).await?;
    entries
        .into_iter()
        .find(|e| e.name == name)
        .ok_or_else(|| format!("entry '{name}' not found").into())
}

fn field_of_type(entry: &Entry, field_type: FieldType) -> Result<&Field> {
    entry
        .fields
        .iter()
        .find(|f| f.field_type == field_type)
        .ok_or_else(|| format!("no field of type {field_type:?}").into())
}

pub(super) fn agent_servable(entry: &Entry) -> Result<&Field> {
    entry
        .fields
        .iter()
        .find(|f| f.key == "agent_servable")
        .ok_or_else(|| "no agent_servable field".into())
}

fn ssh_field_count(entry: &Entry) -> usize {
    entry
        .fields
        .iter()
        .filter(|f| {
            matches!(
                f.field_type,
                FieldType::SshPrivateKey
                    | FieldType::SshPublicKey
                    | FieldType::SshKeyType
                    | FieldType::SshComment
            )
        })
        .count()
}

pub(super) fn ssh_entry_with_private_pem(name: &str, pem: &str) -> Entry {
    Entry {
        entry_type: EntryType::SshKey,
        ..test_entry(
            Uuid::nil(),
            name,
            vec![test_field(
                "private_key",
                FieldValue::Hidden(pem.to_string()),
                FieldType::SshPrivateKey,
                true,
                0,
            )],
        )
    }
}

fn fingerprint_of(public_line: &str) -> Result<String> {
    Ok(PublicKey::from_openssh(public_line)?
        .fingerprint(HashAlg::Sha256)
        .to_string())
}

// ── keygen ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn keygen_creates_ssh_entry_with_four_fields_and_agent_servable() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let (registry, identity, vault) = make_registry(&db, &[]).await?;

    // -- Exec
    let output = handle_keygen(
        Some("alice@laptop".to_string()),
        None,
        None,
        registry,
        &identity,
    )
    .await?;

    // -- Check
    assert_eq!(output.name, "alice@laptop");
    let entry = reload(&db, vault.id, &identity, "alice@laptop").await?;
    assert_eq!(entry.entry_type, EntryType::SshKey);
    assert_eq!(ssh_field_count(&entry), 4);
    assert_eq!(entry.fields.len(), 5);

    let private = field_of_type(&entry, FieldType::SshPrivateKey)?;
    assert!(private.encrypted, "private key field is encrypted");
    assert!(matches!(private.value, FieldValue::Hidden(_)));
    assert!(private.value.to_string().starts_with("-----BEGIN OPENSSH"));

    let public = field_of_type(&entry, FieldType::SshPublicKey)?;
    assert!(!public.encrypted);
    assert_eq!(public.value.to_string(), output.public_key);

    let key_type = field_of_type(&entry, FieldType::SshKeyType)?;
    assert_eq!(key_type.value.to_string(), "Ed25519");

    let comment = field_of_type(&entry, FieldType::SshComment)?;
    assert_eq!(comment.value.to_string(), "alice@laptop");

    let servable = agent_servable(&entry)?;
    assert_eq!(servable.value, FieldValue::Boolean(false));
    assert!(!servable.encrypted, "agent_servable is plaintext");
    Ok(())
}

#[tokio::test]
async fn keygen_prints_fingerprint_of_stored_public_key() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let (registry, identity, vault) = make_registry(&db, &[]).await?;

    // -- Exec
    let output = handle_keygen(
        Some("bob@laptop".to_string()),
        None,
        None,
        registry,
        &identity,
    )
    .await?;

    // -- Check
    let entry = reload(&db, vault.id, &identity, "bob@laptop").await?;
    let stored = field_of_type(&entry, FieldType::SshPublicKey)?
        .value
        .to_string();
    assert_eq!(output.public_key, stored);
    assert_eq!(output.fingerprint, fingerprint_of(&stored)?);
    Ok(())
}

#[tokio::test]
async fn keygen_uses_explicit_entry_name() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let (registry, identity, vault) = make_registry(&db, &[]).await?;

    // -- Exec
    let output = handle_keygen(
        Some("carol@work".to_string()),
        Some("work-key".to_string()),
        None,
        registry,
        &identity,
    )
    .await?;

    // -- Check
    assert_eq!(output.name, "work-key");
    let entry = reload(&db, vault.id, &identity, "work-key").await?;
    assert_eq!(
        field_of_type(&entry, FieldType::SshComment)?
            .value
            .to_string(),
        "carol@work"
    );
    Ok(())
}

#[tokio::test]
async fn keygen_rejects_duplicate_entry_name() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let (registry, identity, _vault) = make_registry(&db, &[]).await?;
    handle_keygen(
        Some("dave@host".to_string()),
        None,
        None,
        registry.clone(),
        &identity,
    )
    .await?;

    // -- Exec
    let result = handle_keygen(
        Some("dave@host".to_string()),
        None,
        None,
        registry,
        &identity,
    )
    .await;

    // -- Check
    assert!(result.is_err(), "duplicate entry name must be rejected");
    Ok(())
}

// ── rotate ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn rotate_replaces_private_and_public_key() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let (registry, identity, vault) = make_registry(&db, &[]).await?;
    let created = handle_keygen(
        Some("erin@host".to_string()),
        None,
        None,
        registry.clone(),
        &identity,
    )
    .await?;
    let before = reload(&db, vault.id, &identity, "erin@host").await?;
    let retired_pem = field_of_type(&before, FieldType::SshPrivateKey)?
        .value
        .to_string();
    let retired_public = field_of_type(&before, FieldType::SshPublicKey)?
        .value
        .to_string();
    assert_eq!(created.fingerprint, fingerprint_of(&retired_public)?);

    // -- Exec
    let rotated = handle_rotate("erin@host".to_string(), None, None, registry, &identity).await?;

    // -- Check
    let after = reload(&db, vault.id, &identity, "erin@host").await?;
    let new_pem = field_of_type(&after, FieldType::SshPrivateKey)?
        .value
        .to_string();
    let new_public = field_of_type(&after, FieldType::SshPublicKey)?
        .value
        .to_string();
    assert_ne!(new_pem, retired_pem);
    assert_ne!(new_public, retired_public);
    assert_eq!(rotated.fingerprint, fingerprint_of(&new_public)?);
    assert_ne!(rotated.fingerprint, created.fingerprint);
    assert!(
        !after
            .fields
            .iter()
            .any(|f| f.value.to_string() == retired_pem),
        "retired private key must not survive in any field"
    );
    assert_eq!(ssh_field_count(&after), 4);
    assert_eq!(after.fields.len(), 5, "agent_servable survives rotation");
    assert_eq!(
        field_of_type(&after, FieldType::SshComment)?
            .value
            .to_string(),
        "erin@host"
    );
    Ok(())
}

#[tokio::test]
async fn rotate_updates_comment_when_given() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let (registry, identity, vault) = make_registry(&db, &[]).await?;
    handle_keygen(
        Some("frank@host".to_string()),
        None,
        None,
        registry.clone(),
        &identity,
    )
    .await?;

    // -- Exec
    handle_rotate(
        "frank@host".to_string(),
        Some("frank@newhost".to_string()),
        None,
        registry,
        &identity,
    )
    .await?;

    // -- Check
    let after = reload(&db, vault.id, &identity, "frank@host").await?;
    assert_eq!(
        field_of_type(&after, FieldType::SshComment)?
            .value
            .to_string(),
        "frank@newhost"
    );
    let public = field_of_type(&after, FieldType::SshPublicKey)?
        .value
        .to_string();
    assert!(public.ends_with("frank@newhost"));
    Ok(())
}

#[tokio::test]
async fn rotate_fills_missing_ssh_fields() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let fixture = ssh_entry_with_private_pem("legacy", "free text from the TUI");
    let (registry, identity, vault) = make_registry(&db, &[fixture]).await?;

    // -- Exec
    handle_rotate("legacy".to_string(), None, None, registry, &identity).await?;

    // -- Check
    let after = reload(&db, vault.id, &identity, "legacy").await?;
    assert_eq!(ssh_field_count(&after), 4);
    let private = field_of_type(&after, FieldType::SshPrivateKey)?
        .value
        .to_string();
    assert!(private.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
    assert_eq!(
        field_of_type(&after, FieldType::SshKeyType)?
            .value
            .to_string(),
        "Ed25519"
    );
    Ok(())
}

#[tokio::test]
async fn rotate_unknown_entry_errors() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let (registry, identity, _vault) = make_registry(&db, &[]).await?;

    // -- Exec
    let result = handle_rotate("nope".to_string(), None, None, registry, &identity).await;

    // -- Check
    assert!(result.is_err());
    Ok(())
}

// ── pubkey ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pubkey_returns_stored_public_line() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let (registry, identity, vault) = make_registry(&db, &[]).await?;
    let created = handle_keygen(
        Some("grace@host".to_string()),
        None,
        None,
        registry.clone(),
        &identity,
    )
    .await?;

    // -- Exec
    let line = handle_pubkey("grace@host".to_string(), None, registry, &identity).await?;

    // -- Check
    let entry = reload(&db, vault.id, &identity, "grace@host").await?;
    assert_eq!(
        line,
        field_of_type(&entry, FieldType::SshPublicKey)?
            .value
            .to_string()
    );
    assert_eq!(line, created.public_key);
    assert!(line.starts_with("ssh-ed25519 "));
    Ok(())
}

#[tokio::test]
async fn pubkey_rejects_invalid_private_pem() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let fixture = ssh_entry_with_private_pem("broken", "not a key");
    let (registry, identity, _vault) = make_registry(&db, &[fixture]).await?;

    // -- Exec
    let result = handle_pubkey("broken".to_string(), None, registry, &identity).await;

    // -- Check
    let message = match result {
        Err(e) => e.to_string(),
        Ok(line) => return Err(format!("expected an error, got '{line}'").into()),
    };
    assert!(
        message.contains("invalid openssh pem"),
        "unexpected message: {message}"
    );
    Ok(())
}

#[tokio::test]
async fn pubkey_unknown_entry_errors() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("test.db");
    let (registry, identity, _vault) = make_registry(&db, &[]).await?;

    // -- Exec
    let result = handle_pubkey("nope".to_string(), None, registry, &identity).await;

    // -- Check
    assert!(result.is_err());
    Ok(())
}
