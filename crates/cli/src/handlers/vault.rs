//! Vault CLI command handlers.

use crate::args::VaultCommands;
use bogita_core::domain::Vault;
use bogita_core::error::{DbError, Error, Result};
use bogita_core::ports::{Crypto, Storage};
use bogita_core::vault::registry::VaultRegistry;

pub enum VaultOutput {
    List(Vec<Vault>),
    Ok,
}

pub async fn handle_vault<S, C>(
    cmd: VaultCommands,
    registry: VaultRegistry<S, C>,
) -> Result<VaultOutput>
where
    S: Storage,
    C: Crypto + Clone,
{
    match cmd {
        VaultCommands::List => {
            let vaults = registry.list_vaults().await?;
            Ok(VaultOutput::List(vaults))
        }
        VaultCommands::Lock { name } => {
            let _ = name;
            // Lock/unlock is session-state — stub until TUI session layer (Phase 5)
            Ok(VaultOutput::Ok)
        }
        VaultCommands::Unlock { name } => {
            let _ = name;
            Ok(VaultOutput::Ok)
        }
        VaultCommands::Sync { name } => {
            let _ = name;
            // Git sync stub — backend not yet implemented
            Ok(VaultOutput::Ok)
        }
        VaultCommands::Default { name } => {
            let vaults = registry.list_vaults().await?;
            match vaults.iter().find(|v| v.name == name) {
                Some(vault) => {
                    registry.set_default(vault.id).await?;
                    Ok(VaultOutput::Ok)
                }
                None => Err(Error::Database(DbError::Query(format!(
                    "vault '{name}' not found"
                )))),
            }
        }
        VaultCommands::Add { .. } => unreachable!("vault add is handled as TUI mutation"),
    }
}
