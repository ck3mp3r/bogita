use super::args::{AgentCommands, Cli, Commands, EntryCommands, VaultCommands};
use clap::Parser;

fn parse(args: &[&str]) -> Cli {
    Cli::parse_from(std::iter::once("bogita").chain(args.iter().copied()))
}

// ── no subcommand ─────────────────────────────────────────────────────────

#[test]
fn no_subcommand_parses() {
    let cli = parse(&[]);
    assert!(cli.command.is_none());
}

// ── entry ls / get / search ───────────────────────────────────────────────

#[test]
fn entry_ls_no_vault_parses() {
    let cli = parse(&["entry", "ls"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Ls { vault: None }))
    ));
}

#[test]
fn entry_ls_with_vault_parses() {
    let cli = parse(&["entry", "ls", "--vault", "personal"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Ls { vault: Some(ref v) })) if v == "personal"
    ));
}

#[test]
fn entry_get_parses() {
    let cli = parse(&["entry", "get", "github", "--vault", "personal"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Get {
            ref name,
            field: None,
            vault: Some(ref v),
        })) if name == "github" && v == "personal"
    ));
}

#[test]
fn entry_get_no_vault_parses() {
    let cli = parse(&["entry", "get", "github"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Get {
            ref name,
            field: None,
            vault: None,
        })) if name == "github"
    ));
}

#[test]
fn entry_get_with_field_parses() {
    let cli = parse(&["entry", "get", "github", "--field", "password"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Get {
            ref name,
            field: Some(ref f),
            vault: None,
        })) if name == "github" && f == "password"
    ));
}

#[test]
fn entry_search_parses() {
    let cli = parse(&["entry", "search", "aws", "--vault", "work"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Search {
            ref query,
            vault: Some(ref v),
        })) if query == "aws" && v == "work"
    ));
}

// ── entry TUI mutations ───────────────────────────────────────────────────

#[test]
fn entry_add_no_args_parses() {
    let cli = parse(&["entry", "add"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Add {
            name: None,
            vault: None,
        }))
    ));
}

#[test]
fn entry_add_with_name_parses() {
    let cli = parse(&["entry", "add", "github"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Add {
            name: Some(ref n),
            vault: None,
        })) if n == "github"
    ));
}

#[test]
fn entry_add_with_vault_parses() {
    let cli = parse(&["entry", "add", "github", "--vault", "personal"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Add {
            name: Some(ref n),
            vault: Some(ref v),
        })) if n == "github" && v == "personal"
    ));
}

#[test]
fn entry_edit_parses() {
    let cli = parse(&["entry", "edit", "github"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Edit {
            ref name,
            vault: None,
        })) if name == "github"
    ));
}

#[test]
fn entry_rm_parses() {
    let cli = parse(&["entry", "rm", "github"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Rm {
            ref name,
            vault: None,
        })) if name == "github"
    ));
}

// ── vault ─────────────────────────────────────────────────────────────────

#[test]
fn vault_list_parses() {
    let cli = parse(&["vault", "list"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Vault(VaultCommands::List))
    ));
}

#[test]
fn vault_lock_parses() {
    let cli = parse(&["vault", "lock", "personal"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Vault(VaultCommands::Lock { ref name })) if name == "personal"
    ));
}

#[test]
fn vault_unlock_parses() {
    let cli = parse(&["vault", "unlock", "personal"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Vault(VaultCommands::Unlock { ref name })) if name == "personal"
    ));
}

#[test]
fn vault_sync_no_name_parses() {
    let cli = parse(&["vault", "sync"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Vault(VaultCommands::Sync { name: None }))
    ));
}

#[test]
fn vault_sync_with_name_parses() {
    let cli = parse(&["vault", "sync", "work"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Vault(VaultCommands::Sync { name: Some(ref n) })) if n == "work"
    ));
}

#[test]
fn vault_default_parses() {
    let cli = parse(&["vault", "default", "personal"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Vault(VaultCommands::Default { ref name })) if name == "personal"
    ));
}

#[test]
fn vault_add_parses() {
    let cli = parse(&["vault", "add", "work"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Vault(VaultCommands::Add { ref name })) if name == "work"
    ));
}

// ── agent ─────────────────────────────────────────────────────────────────

#[test]
fn agent_start_parses() {
    let cli = parse(&["agent", "start"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Agent(AgentCommands::Start { socket: None }))
    ));
}

#[test]
fn agent_stop_parses() {
    let cli = parse(&["agent", "stop"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Agent(AgentCommands::Stop))
    ));
}

#[test]
fn agent_status_parses() {
    let cli = parse(&["agent", "status"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Agent(AgentCommands::Status))
    ));
}

#[test]
fn agent_keys_parses() {
    let cli = parse(&["agent", "keys"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Agent(AgentCommands::Keys))
    ));
}
