use crate::handlers::entry::{handle_get, handle_ls, handle_search, EntryOutput, GetOutput};
use crate::test_support::make_registry;
use bogita_core::domain::{Entry, FieldType, FieldValue};
use bogita_core::test_support::{test_entry, test_field, test_password_entry};
use uuid::Uuid;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

fn user_entry(name: &str, username: &str) -> Entry {
    test_entry(
        Uuid::nil(),
        name,
        vec![test_field(
            "username",
            FieldValue::Text(username.to_string()),
            FieldType::Username,
            false,
            0,
        )],
    )
}

// ── ls ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn ls_default_vault_returns_entries() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let entries = [test_password_entry(Uuid::nil(), "GitHub")];
    let (registry, identity, _vault) = make_registry(&dir.path().join("test.db"), &entries).await?;

    // -- Exec
    let output = handle_ls(None, registry, &identity).await?;

    // -- Check
    let EntryOutput::List(entries) = output;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "GitHub");
    Ok(())
}

#[tokio::test]
async fn ls_named_vault_returns_entries() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let entries = [test_password_entry(Uuid::nil(), "GitHub")];
    let (registry, identity, _vault) = make_registry(&dir.path().join("test.db"), &entries).await?;

    // -- Exec
    let output = handle_ls(Some("Personal".to_string()), registry, &identity).await?;

    // -- Check
    let EntryOutput::List(entries) = output;
    assert!(!entries.is_empty());
    Ok(())
}

#[tokio::test]
async fn ls_unknown_vault_errors() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (registry, identity, _vault) = make_registry(&dir.path().join("test.db"), &[]).await?;

    // -- Exec
    let result = handle_ls(Some("nonexistent".to_string()), registry, &identity).await;

    // -- Check
    assert!(result.is_err());
    Ok(())
}

// ── get ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_returns_entry_by_name() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let entries = [test_password_entry(Uuid::nil(), "GitHub")];
    let (registry, identity, _vault) = make_registry(&dir.path().join("test.db"), &entries).await?;

    // -- Exec
    let output = handle_get("GitHub".to_string(), None, None, registry, &identity).await?;

    // -- Check
    match output {
        GetOutput::Entry(entry) => assert_eq!(entry.name, "GitHub"),
        GetOutput::Field(_) => panic!("expected GetOutput::Entry"),
    }
    Ok(())
}

#[tokio::test]
async fn get_unknown_entry_errors() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (registry, identity, _vault) = make_registry(&dir.path().join("test.db"), &[]).await?;

    // -- Exec
    let result = handle_get(
        "does-not-exist".to_string(),
        None,
        None,
        registry,
        &identity,
    )
    .await;

    // -- Check
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn get_field_returns_value() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let entries = [test_password_entry(Uuid::nil(), "GitHub")];
    let (registry, identity, _vault) = make_registry(&dir.path().join("test.db"), &entries).await?;

    // -- Exec
    let output = handle_get(
        "GitHub".to_string(),
        Some("username".to_string()),
        None,
        registry,
        &identity,
    )
    .await?;

    // -- Check
    match output {
        GetOutput::Field(value) => assert_eq!(value, "alice"),
        GetOutput::Entry(_) => panic!("expected GetOutput::Field"),
    }
    Ok(())
}

#[tokio::test]
async fn get_unknown_field_errors() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let entries = [test_password_entry(Uuid::nil(), "GitHub")];
    let (registry, identity, _vault) = make_registry(&dir.path().join("test.db"), &entries).await?;

    // -- Exec
    let result = handle_get(
        "GitHub".to_string(),
        Some("does-not-exist".to_string()),
        None,
        registry,
        &identity,
    )
    .await;

    // -- Check
    assert!(result.is_err());
    Ok(())
}

// ── search ────────────────────────────────────────────────────────────────

fn multi_entries() -> Vec<Entry> {
    vec![
        user_entry("GitHub", "alice"),
        user_entry("Gitlab", "bob"),
        user_entry("AWS", "carol"),
    ]
}

#[tokio::test]
async fn search_returns_matching_entries() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let entries = multi_entries();
    let (registry, identity, _vault) = make_registry(&dir.path().join("test.db"), &entries).await?;

    // -- Exec
    let EntryOutput::List(entries) =
        handle_search("git".to_string(), None, registry, &identity).await?;

    // -- Check
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e| e.name == "GitHub"));
    assert!(entries.iter().any(|e| e.name == "Gitlab"));
    Ok(())
}

#[tokio::test]
async fn search_no_match_returns_empty() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let entries = multi_entries();
    let (registry, identity, _vault) = make_registry(&dir.path().join("test.db"), &entries).await?;

    // -- Exec
    let EntryOutput::List(entries) =
        handle_search("zzz-no-match".to_string(), None, registry, &identity).await?;

    // -- Check
    assert!(entries.is_empty());
    Ok(())
}

#[tokio::test]
async fn search_named_vault() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let entries = multi_entries();
    let (registry, identity, _vault) = make_registry(&dir.path().join("test.db"), &entries).await?;

    // -- Exec
    let EntryOutput::List(entries) = handle_search(
        "AWS".to_string(),
        Some("Personal".to_string()),
        registry,
        &identity,
    )
    .await?;

    // -- Check
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "AWS");
    Ok(())
}

#[tokio::test]
async fn search_unknown_vault_errors() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let entries = multi_entries();
    let (registry, identity, _vault) = make_registry(&dir.path().join("test.db"), &entries).await?;

    // -- Exec
    let result = handle_search(
        "anything".to_string(),
        Some("nonexistent".to_string()),
        registry,
        &identity,
    )
    .await;

    // -- Check
    assert!(result.is_err());
    Ok(())
}
