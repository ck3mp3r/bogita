use crate::exit_with_error;
use bogita_cli::args::CliCommand;
use bogita_cli::handlers::entry::{
    handle_get, handle_keygen, handle_ls, handle_pubkey, handle_rotate, handle_search,
    handle_serve, EntryOutput, GetOutput,
};
use bogita_cli::handlers::vault::{handle_default, handle_list, handle_rm};
use bogita_core::app::DefaultApp;
use bogita_core::domain::{FieldType, FieldValue, SecretString};

/// Returns `true` if a command requires the vault to be unlocked (identity loaded).
fn command_needs_identity(cmd: &CliCommand) -> bool {
    matches!(
        cmd,
        CliCommand::EntryLs { .. }
            | CliCommand::EntryGet { .. }
            | CliCommand::EntrySearch { .. }
            | CliCommand::EntryCp { .. }
            | CliCommand::EntryKeygen { .. }
            | CliCommand::EntryRotate { .. }
            | CliCommand::EntryPubkey { .. }
            | CliCommand::EntryServe { .. }
    )
}

pub(crate) async fn run_cli(cmd: CliCommand, app: DefaultApp) {
    if app.is_locked && command_needs_identity(&cmd) {
        eprintln!("error: vault is locked — run `bogita unlock`");
        return;
    }

    let identity = match app.identity.as_ref() {
        Some(id) => id,
        None => {
            eprintln!("error: vault is locked — run `bogita unlock`");
            return;
        }
    };

    match cmd {
        CliCommand::EntryLs { vault } => match handle_ls(vault, app.registry, identity).await {
            Ok(EntryOutput::List(entries)) => {
                for entry in entries {
                    println!("{}", entry.name);
                }
            }
            Err(e) => eprintln!("error: {e}"),
        },
        CliCommand::EntryGet { name, field, vault } => {
            match handle_get(name, field, vault, app.registry, identity).await {
                Ok(GetOutput::Entry(entry)) => {
                    println!("name: {}", entry.name);
                    println!("type: {:?}", entry.entry_type);
                    for field in &entry.fields {
                        let value = if field.field_type == FieldType::TotpSecret
                            || matches!(field.value, FieldValue::Hidden(_))
                        {
                            "****".to_string()
                        } else {
                            field.value.to_string()
                        };
                        println!("  {}: {}", field.key, value);
                    }
                }
                Ok(GetOutput::Field(value)) => println!("{}", value),
                Err(e) => eprintln!("error: {e}"),
            }
        }
        CliCommand::EntrySearch { query, vault } => {
            match handle_search(query, vault, app.registry, identity).await {
                Ok(EntryOutput::List(entries)) => {
                    for entry in entries {
                        println!("{}", entry.name);
                    }
                }
                Err(e) => eprintln!("error: {e}"),
            }
        }
        CliCommand::EntryCp { name, field, vault } => {
            match handle_get(name, Some(field), vault, app.registry, identity).await {
                Ok(GetOutput::Field(value)) => copy_to_clipboard(value).await,
                Ok(GetOutput::Entry(_)) => eprintln!("error: field value not resolved"),
                Err(e) => eprintln!("error: {e}"),
            }
        }
        CliCommand::EntryKeygen {
            comment,
            name,
            vault,
        } => match handle_keygen(comment, name, vault, app.registry, identity).await {
            Ok(output) => {
                println!("created entry '{}'", output.name);
                println!("fingerprint: {}", output.fingerprint);
                println!("{}", output.public_key);
            }
            Err(e) => exit_with_error(&e.to_string()),
        },
        CliCommand::EntryRotate {
            name,
            comment,
            vault,
        } => match handle_rotate(name, comment, vault, app.registry, identity).await {
            Ok(output) => {
                println!("rotated entry '{}'", output.name);
                println!("fingerprint: {}", output.fingerprint);
                println!("{}", output.public_key);
            }
            Err(e) => exit_with_error(&e.to_string()),
        },
        CliCommand::EntryPubkey { name, copy, vault } => {
            match handle_pubkey(name, vault, app.registry, identity).await {
                Ok(line) => {
                    if copy {
                        copy_to_clipboard(line).await;
                    } else {
                        println!("{line}");
                    }
                }
                Err(e) => exit_with_error(&e.to_string()),
            }
        }
        CliCommand::EntryServe { name, on, vault } => {
            match handle_serve(name, on, vault, app.registry, identity).await {
                Ok(()) => println!("agent_servable = {}", if on { "on" } else { "off" }),
                Err(e) => exit_with_error(&e.to_string()),
            }
        }
        CliCommand::VaultList => match handle_list(app.registry).await {
            Ok(vaults) => {
                for vault in vaults {
                    let marker = if vault.is_default { "*" } else { " " };
                    println!("{} {}", marker, vault.name);
                }
            }
            Err(e) => eprintln!("error: {e}"),
        },
        CliCommand::VaultDefault { name } => match handle_default(&name, app.registry).await {
            Ok(()) => println!("default vault set"),
            Err(e) => eprintln!("error: {e}"),
        },
        CliCommand::VaultRm { name } => match handle_rm(&name, app.registry).await {
            Ok(()) => println!("vault '{}' removed", name),
            Err(e) => eprintln!("error: {e}"),
        },
    }
}

/// Copy a secret to the clipboard; it is cleared after `CLIPBOARD_TIMEOUT_SECS`.
async fn copy_to_clipboard(value: String) {
    use bogita_core::service::clipboard::{system, CLIPBOARD_TIMEOUT_SECS};
    match system()
        .copy_with_timeout(SecretString::from(value), CLIPBOARD_TIMEOUT_SECS)
        .await
    {
        Ok(()) => eprintln!("copied to clipboard (clears in {CLIPBOARD_TIMEOUT_SECS}s)"),
        Err(e) => eprintln!("clipboard error: {e}"),
    }
}
