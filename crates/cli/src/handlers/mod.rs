pub mod agent;
pub mod entry;
pub mod ssh_relay;
pub mod ssh_sign;
pub mod vault;

use bogita_core::domain::Vault;
use bogita_core::error::{DbError, Error, Result};
use bogita_core::ports::VaultStore;
use bogita_core::vault::registry::VaultRegistry;

/// Resolve the target vault: use `--vault <name>` if supplied, else the default vault.
pub(crate) async fn resolve_vault<S>(
    vault_name: &Option<String>,
    registry: &VaultRegistry<S>,
) -> Result<Vault>
where
    S: VaultStore,
{
    match vault_name {
        Some(name) => {
            let vaults = registry.list_vaults().await?;
            vaults
                .into_iter()
                .find(|v| &v.name == name)
                .ok_or_else(|| Error::Database(DbError::Query(format!("vault '{name}' not found"))))
        }
        None => registry
            .default_vault()
            .await?
            .ok_or_else(|| Error::Database(DbError::Query("no default vault set".to_string()))),
    }
}
