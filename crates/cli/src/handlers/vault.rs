//! Vault CLI command handlers.

use bogita_core::domain::Vault;
use bogita_core::error::Result;
use bogita_core::ports::VaultStore;
use bogita_core::vault::registry::VaultRegistry;

use super::resolve_vault;

/// `bogita vault list`
pub async fn handle_list<S>(registry: VaultRegistry<S>) -> Result<Vec<Vault>>
where
    S: VaultStore,
{
    registry.list_vaults().await
}

/// `bogita vault default <name>`
pub async fn handle_default<S>(name: &str, registry: VaultRegistry<S>) -> Result<()>
where
    S: VaultStore,
{
    let vault = resolve_vault(&Some(name.to_string()), &registry).await?;
    registry.set_default(vault.id).await
}

/// `bogita vault rm <name>`
pub async fn handle_rm<S>(name: &str, registry: VaultRegistry<S>) -> Result<()>
where
    S: VaultStore,
{
    let vault = resolve_vault(&Some(name.to_string()), &registry).await?;
    registry.remove_vault(vault.id).await
}

// region:    --- Tests
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/handlers/vault.rs"]
mod vault_test;
// endregion: --- Tests
