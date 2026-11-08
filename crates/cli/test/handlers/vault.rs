use crate::handlers::vault::{handle_default, handle_list, handle_rm};
use crate::test_support::make_registry;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::test]
async fn vault_list_returns_vaults() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (registry, _identity, _vault) = make_registry(&dir.path().join("test.db"), &[]).await?;

    // -- Exec
    let vaults = handle_list(registry).await?;

    // -- Check
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].name, "Personal");
    Ok(())
}

#[tokio::test]
async fn vault_default_sets_default() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (registry, _identity, _vault) = make_registry(&dir.path().join("test.db"), &[]).await?;

    // -- Exec
    handle_default("Personal", registry).await?;

    // -- Check
    Ok(())
}

#[tokio::test]
async fn vault_default_unknown_vault_errors() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (registry, _identity, _vault) = make_registry(&dir.path().join("test.db"), &[]).await?;

    // -- Exec
    let result = handle_default("nonexistent", registry).await;

    // -- Check
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn vault_rm_removes_vault() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (registry, _identity, _vault) = make_registry(&dir.path().join("test.db"), &[]).await?;

    // -- Exec
    handle_rm("Personal", registry.clone()).await?;

    // -- Check
    let vaults = registry.list_vaults().await?;
    assert!(vaults.is_empty());
    Ok(())
}

#[tokio::test]
async fn vault_rm_unknown_errors() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let (registry, _identity, _vault) = make_registry(&dir.path().join("test.db"), &[]).await?;

    // -- Exec
    let result = handle_rm("nonexistent", registry).await;

    // -- Check
    assert!(result.is_err());
    Ok(())
}
