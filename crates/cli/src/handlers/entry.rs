//! Entry CLI command handlers (read-only: ls, get, search; SSH key workflows).

use bogita_core::agent::keyring::{algorithm_supported, KeyRing};
use bogita_core::domain::{
    agent_servable_field, AgeIdentity, Entry, EntryType, FieldType, FieldValue, SshKeyMaterial,
    Uuid, AGENT_SERVABLE_KEY,
};
use bogita_core::error::{DbError, Error, Result, ValidationError};
use bogita_core::ports::Storage;
use bogita_core::vault::registry::VaultRegistry;
use bogita_core::vault::service::VaultEntryService;

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

// ── SSH key workflows ─────────────────────────────────────────────────────

/// Output of `entry keygen` and `entry rotate`.
pub struct SshKeyOutput {
    /// Entry name — the handle `rotate`, `pubkey`, and `serve` take
    pub name: String,
    /// Public key line in wire format
    pub public_key: String,
    /// SHA-256 fingerprint as printed by `ssh-keygen -l`
    pub fingerprint: String,
}

/// `bogita entry keygen [comment] [--name <name>] [--vault <name>]`
///
/// Generates an Ed25519 key pair and stores it as an `SshKey` entry: the
/// private PEM (encrypted), the public line, the key type, and the comment,
/// plus a plaintext `agent_servable` boolean that defaults to `false`.
pub async fn handle_keygen<S>(
    comment: Option<String>,
    name: Option<String>,
    vault_name: Option<String>,
    registry: VaultRegistry<S>,
    identity: &AgeIdentity,
) -> Result<SshKeyOutput>
where
    S: Storage,
{
    let vault = resolve_vault(&vault_name, &registry).await?;
    let svc = registry.vault_service_for(&vault, identity.clone());

    let comment = comment.unwrap_or_default();
    let entry_name = name
        .filter(|n| !n.is_empty())
        .or_else(|| (!comment.is_empty()).then(|| comment.clone()))
        .unwrap_or_else(|| "ssh-key".to_string());

    let name_taken = svc
        .list_entries(vault.id, None)
        .await?
        .iter()
        .any(|e| e.name == entry_name);
    if name_taken {
        return Err(Error::Database(DbError::Query(format!(
            "entry '{entry_name}' already exists"
        ))));
    }

    let material = SshKeyMaterial::keygen_ed25519(comment)?;
    let mut fields = material.to_fields();
    let servable_idx = fields.len() as i32;
    fields.push(agent_servable_field(false, servable_idx));

    let now = now_secs();
    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: vault.id,
        name: entry_name.clone(),
        entry_type: EntryType::SshKey,
        created_at: now,
        modified_at: now,
        fields,
    };
    svc.save_entry(&entry).await?;

    Ok(SshKeyOutput {
        name: entry_name,
        public_key: material.public_line().to_string(),
        fingerprint: material.fingerprint().to_string(),
    })
}

/// `bogita entry rotate <name> [--comment <comment>] [--vault <name>]`
///
/// Replaces the private and public key of an `SshKey` entry in one save. The
/// existing comment is kept unless `--comment` is given.
pub async fn handle_rotate<S>(
    name: String,
    comment: Option<String>,
    vault_name: Option<String>,
    registry: VaultRegistry<S>,
    identity: &AgeIdentity,
) -> Result<SshKeyOutput>
where
    S: Storage,
{
    let vault = resolve_vault(&vault_name, &registry).await?;
    let svc = registry.vault_service_for(&vault, identity.clone());
    let mut entry = find_entry(&svc, vault.id, &name).await?;

    if entry.entry_type != EntryType::SshKey {
        return Err(Error::Database(DbError::Query(format!(
            "entry '{name}' is not an SSH key entry"
        ))));
    }

    let comment = comment
        .or_else(|| field_value(&entry, &FieldType::SshComment))
        .unwrap_or_default();
    let material = SshKeyMaterial::keygen_ed25519(comment)?;

    material.apply_to(&mut entry.fields);
    entry.modified_at = now_secs();
    svc.save_entry(&entry).await?;

    Ok(SshKeyOutput {
        name: entry.name,
        public_key: material.public_line().to_string(),
        fingerprint: material.fingerprint().to_string(),
    })
}

/// `bogita entry pubkey <name> [--copy] [--vault <name>]`
///
/// Returns the public key line. It is derived from the stored private PEM
/// when the entry has one, so a corrupt PEM fails validation instead of
/// quietly serving a stale public key.
pub async fn handle_pubkey<S>(
    name: String,
    vault_name: Option<String>,
    registry: VaultRegistry<S>,
    identity: &AgeIdentity,
) -> Result<String>
where
    S: Storage,
{
    let vault = resolve_vault(&vault_name, &registry).await?;
    let svc = registry.vault_service_for(&vault, identity.clone());
    let entry = find_entry(&svc, vault.id, &name).await?;

    if let Some(pem) = field_value(&entry, &FieldType::SshPrivateKey) {
        let material = SshKeyMaterial::validate(pem)?;
        return Ok(material.public_line().to_string());
    }

    field_value(&entry, &FieldType::SshPublicKey).ok_or_else(|| {
        Error::Database(DbError::Query(format!(
            "entry '{name}' has no SSH public key"
        )))
    })
}

/// `bogita entry serve <name> --on|--off [--vault <name>]`
///
/// Sets the plaintext `agent_servable` boolean that the agent daemon reads.
///
/// Opting in validates the stored PEM before it flips the boolean: a value that
/// does not parse, or a key the daemon cannot sign with, is refused here by
/// name instead of being advertised as servable and then skipped at load.
/// Toggling serving off never needs a parsable key.
pub async fn handle_serve<S>(
    name: String,
    on: bool,
    vault_name: Option<String>,
    registry: VaultRegistry<S>,
    identity: &AgeIdentity,
) -> Result<()>
where
    S: Storage,
{
    let vault = resolve_vault(&vault_name, &registry).await?;
    let svc = registry.vault_service_for(&vault, identity.clone());
    let mut entry = find_entry(&svc, vault.id, &name).await?;

    if on && entry.entry_type == EntryType::SshKey {
        let Some(pem) = field_value(&entry, &FieldType::SshPrivateKey) else {
            return Err(Error::Database(DbError::Query(format!(
                "entry '{name}' has no private key field to serve"
            ))));
        };
        let material = SshKeyMaterial::validate(pem)?;
        if !algorithm_supported(material.algorithm()) {
            return Err(Error::Validation(
                ValidationError::UnsupportedAgentAlgorithm(KeyRing::unsupported_algorithm_message(
                    &entry.name,
                    material.algorithm(),
                )),
            ));
        }
    }

    let existing = entry
        .fields
        .iter()
        .position(|f| f.key == AGENT_SERVABLE_KEY);
    match existing {
        Some(position) => {
            entry.fields[position].value = FieldValue::Boolean(on);
            entry.fields[position].encrypted = false;
            if entry.fields[position].field_type
                == FieldType::Custom(AGENT_SERVABLE_KEY.to_string())
            {
                entry.fields[position].field_type = FieldType::Boolean;
            }
        }
        None => {
            let idx = entry.fields.len() as i32;
            entry.fields.push(agent_servable_field(on, idx));
        }
    }

    entry.modified_at = now_secs();
    svc.save_entry(&entry).await
}

/// Find one entry by name in the resolved vault.
async fn find_entry<S>(svc: &VaultEntryService<&S>, vault_id: Uuid, name: &str) -> Result<Entry>
where
    S: Storage,
{
    svc.list_entries(vault_id, None)
        .await?
        .into_iter()
        .find(|e| e.name == name)
        .ok_or_else(|| Error::Database(DbError::Query(format!("entry '{name}' not found"))))
}

/// Read one field value by field type.
fn field_value(entry: &Entry, field_type: &FieldType) -> Option<String> {
    entry
        .fields
        .iter()
        .find(|f| &f.field_type == field_type)
        .map(|f| f.value.to_string())
}

/// Current Unix timestamp in seconds.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// region:    --- Tests
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/handlers/entry.rs"]
mod entry_test;

#[cfg(all(test, feature = "integration"))]
#[path = "../../test/handlers/entry_ssh.rs"]
mod entry_ssh_test;

#[cfg(all(test, feature = "integration"))]
#[path = "../../test/handlers/entry_ssh/serve.rs"]
mod entry_ssh_serve_test;
// endregion: --- Tests
