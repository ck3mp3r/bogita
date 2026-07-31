//! Entry CLI command handlers (read-only: ls, get, search).

use crate::args::EntryCommands;
use bogita_core::domain::{AgeIdentity, Entry, FieldType, FieldValue};
use bogita_core::error::{DbError, Error, Result};
use bogita_core::ports::{Crypto, Storage};
use bogita_core::vault::registry::VaultRegistry;

pub enum EntryOutput {
    /// `ls` / `search` — list of entries
    List(Vec<Entry>),
    /// `get` without --field — full entry
    Entry(Entry),
    /// `get --field` — single resolved field value
    Field(String),
}

/// Resolve the target vault: use `--vault <name>` if supplied, else the default vault.
async fn resolve_vault<S, C>(
    vault_name: &Option<String>,
    registry: &VaultRegistry<S, C>,
) -> Result<bogita_core::domain::Vault>
where
    S: Storage,
    C: Crypto + Clone,
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

/// `bogita entry ls [--vault <name>]`
///
/// Lists all entries (all types) in the resolved vault.
pub async fn handle_ls<S, C>(
    cmd: EntryCommands,
    registry: VaultRegistry<S, C>,
    identity: AgeIdentity,
) -> Result<EntryOutput>
where
    S: Storage,
    C: Crypto + Clone,
{
    let vault_name = match &cmd {
        EntryCommands::Ls { vault } => vault,
        _ => unreachable!("handle_ls called with non-Ls command"),
    };

    let vault = resolve_vault(vault_name, &registry).await?;
    let svc = registry.vault_service_for(&vault, identity);
    let entries = svc.list_entries(vault.id, None).await?;
    Ok(EntryOutput::List(entries))
}

/// `bogita entry get <name> [--field <key>] [--vault <name>]`
///
/// Without `--field`: returns the full entry.
/// With `--field`: returns the resolved field value as a string.
///   - `TotpSecret` fields compute and return the live TOTP code instead of the raw secret.
pub async fn handle_get<S, C>(
    cmd: EntryCommands,
    registry: VaultRegistry<S, C>,
    identity: AgeIdentity,
) -> Result<EntryOutput>
where
    S: Storage,
    C: Crypto + Clone,
{
    let (name, field_key, vault_name) = match &cmd {
        EntryCommands::Get { name, field, vault } => (name, field, vault),
        _ => unreachable!("handle_get called with non-Get command"),
    };

    let vault = resolve_vault(vault_name, &registry).await?;
    let svc = registry.vault_service_for(&vault, identity);
    let entries = svc.list_entries(vault.id, None).await?;

    let entry = entries
        .into_iter()
        .find(|e| &e.name == name)
        .ok_or_else(|| Error::Database(DbError::Query(format!("entry '{name}' not found"))))?;

    match field_key {
        None => Ok(EntryOutput::Entry(entry)),
        Some(key) => {
            let field = entry.fields.iter().find(|f| &f.key == key).ok_or_else(|| {
                Error::Database(DbError::Query(format!(
                    "field '{key}' not found in entry '{name}'"
                )))
            })?;

            let value = if field.field_type == FieldType::TotpSecret {
                compute_totp(&entry, field)?
            } else {
                field_value_to_string(&field.value)
            };

            Ok(EntryOutput::Field(value))
        }
    }
}

/// `bogita entry search <query> [--vault <name>]`
///
/// Searches all entries in the resolved vault for the given query string.
pub async fn handle_search<S, C>(
    cmd: EntryCommands,
    registry: VaultRegistry<S, C>,
    identity: AgeIdentity,
) -> Result<EntryOutput>
where
    S: Storage,
    C: Crypto + Clone,
{
    let (query, vault_name) = match &cmd {
        EntryCommands::Search { query, vault } => (query, vault),
        _ => unreachable!("handle_search called with non-Search command"),
    };

    let vault = resolve_vault(vault_name, &registry).await?;
    let svc = registry.vault_service_for(&vault, identity);
    let entries = svc.list_entries(vault.id, Some(query)).await?;
    Ok(EntryOutput::List(entries))
}

/// Compute the current TOTP code from a TotpSecret field and sibling TOTP fields.
fn compute_totp(entry: &Entry, secret_field: &bogita_core::domain::Field) -> Result<String> {
    use bogita_core::service::otp::{decode_secret, generate_totp};
    use std::time::{SystemTime, UNIX_EPOCH};

    let secret = field_value_to_string(&secret_field.value);
    let secret_bytes = decode_secret(&secret)
        .map_err(|e| Error::Database(DbError::Query(format!("TOTP: invalid secret: {e}"))))?;

    let period = entry
        .fields
        .iter()
        .find(|f| f.field_type == FieldType::TotpPeriod)
        .and_then(|f| match &f.value {
            FieldValue::Number(n) => Some(*n as u64),
            FieldValue::Text(s) => s.parse().ok(),
            _ => None,
        })
        .unwrap_or(30);

    let digits = entry
        .fields
        .iter()
        .find(|f| f.field_type == FieldType::TotpDigits)
        .and_then(|f| match &f.value {
            FieldValue::Number(n) => Some(*n as u32),
            FieldValue::Text(s) => s.parse().ok(),
            _ => None,
        })
        .unwrap_or(6);

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let (code, _) = generate_totp(&secret_bytes, period, digits, now_secs)
        .map_err(|e| Error::Database(DbError::Query(format!("TOTP computation failed: {e}"))))?;
    Ok(code)
}

fn field_value_to_string(value: &FieldValue) -> String {
    match value {
        FieldValue::Text(s) | FieldValue::Hidden(s) | FieldValue::Url(s) | FieldValue::Email(s) => {
            s.clone()
        }
        FieldValue::Boolean(b) => b.to_string(),
        FieldValue::Number(n) => n.to_string(),
        FieldValue::Date(ts) => ts.to_string(),
    }
}
