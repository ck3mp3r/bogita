//! Entry CLI command handlers (read-only: ls, get, search).

use bogita_core::domain::{AgeIdentity, Entry, FieldType};
use bogita_core::error::{DbError, Error, Result};
use bogita_core::ports::Storage;
use bogita_core::vault::registry::VaultRegistry;

use super::resolve_vault;

/// List output shared by `ls` and `search`.
pub enum EntryOutput {
    /// `ls` / `search` — list of entries
    List(Vec<Entry>),
}

/// Output of `get`: a whole entry or one resolved field value.
pub enum GetOutput {
    /// `get` without `--field`
    Entry(Entry),
    /// `get --field` — resolved field value
    Field(String),
}

/// `bogita entry ls [--vault <name>]`
///
/// Lists all entries (all types) in the resolved vault.
pub async fn handle_ls<S>(
    vault_name: Option<String>,
    registry: VaultRegistry<S>,
    identity: &AgeIdentity,
) -> Result<EntryOutput>
where
    S: Storage,
{
    let vault = resolve_vault(&vault_name, &registry).await?;
    let svc = registry.vault_service_for(&vault, identity.clone());
    let entries = svc.list_entries(vault.id, None).await?;
    Ok(EntryOutput::List(entries))
}

/// `bogita entry get <name> [--field <key>] [--vault <name>]`
///
/// Without `--field`: returns the full entry.
/// With `--field`: returns the resolved field value as a string.
///   - `TotpSecret` fields compute and return the live TOTP code instead of the raw secret.
pub async fn handle_get<S>(
    name: String,
    field_key: Option<String>,
    vault_name: Option<String>,
    registry: VaultRegistry<S>,
    identity: &AgeIdentity,
) -> Result<GetOutput>
where
    S: Storage,
{
    let vault = resolve_vault(&vault_name, &registry).await?;
    let svc = registry.vault_service_for(&vault, identity.clone());
    let entries = svc.list_entries(vault.id, None).await?;

    let entry = entries
        .into_iter()
        .find(|e| e.name == name)
        .ok_or_else(|| Error::Database(DbError::Query(format!("entry '{name}' not found"))))?;

    match field_key {
        None => Ok(GetOutput::Entry(entry)),
        Some(key) => {
            let field = entry.fields.iter().find(|f| f.key == key).ok_or_else(|| {
                Error::Database(DbError::Query(format!(
                    "field '{key}' not found in entry '{name}'"
                )))
            })?;

            let value = if field.field_type == FieldType::TotpSecret {
                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                entry
                    .totp_code(&field.key, now_secs)
                    .map(|(code, _)| code)
                    .ok_or_else(|| {
                        Error::Database(DbError::Query("TOTP computation failed".to_string()))
                    })?
            } else {
                field.value.to_string()
            };

            Ok(GetOutput::Field(value))
        }
    }
}

/// `bogita entry search <query> [--vault <name>]`
///
/// Searches all entries in the resolved vault for the given query string.
pub async fn handle_search<S>(
    query: String,
    vault_name: Option<String>,
    registry: VaultRegistry<S>,
    identity: &AgeIdentity,
) -> Result<EntryOutput>
where
    S: Storage,
{
    let vault = resolve_vault(&vault_name, &registry).await?;
    let svc = registry.vault_service_for(&vault, identity.clone());
    let entries = svc.list_entries(vault.id, Some(query.as_str())).await?;
    Ok(EntryOutput::List(entries))
}

// region:    --- Tests
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/handlers/entry.rs"]
mod entry_test;
// endregion: --- Tests
