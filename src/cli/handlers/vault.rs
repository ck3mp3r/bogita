//! Vault CLI command handlers.

use crate::cli::args::VaultCommands;
use crate::crypto::age::AgeCrypto;
use crate::domain::Vault;
use crate::error::{DbError, Error, Result};
use crate::storage::sqlite::SqliteStorage;
use crate::vault::registry::VaultRegistry;

pub enum VaultOutput {
    List(Vec<Vault>),
    Ok,
}

pub async fn handle_vault(
    cmd: VaultCommands,
    registry: VaultRegistry<SqliteStorage<AgeCrypto>, AgeCrypto>,
) -> Result<VaultOutput> {
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
