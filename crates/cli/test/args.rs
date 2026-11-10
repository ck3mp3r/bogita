use crate::args::{AgentCommands, Cli, Commands, EntryCommands, VaultCommands};
use clap::{CommandFactory, Parser};

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

#[test]
fn entry_cp_parses() {
    let cli = parse(&["entry", "cp", "github", "--field", "password"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Cp {
            ref name,
            ref field,
            vault: None,
        })) if name == "github" && field == "password"
    ));
}

// ── entry ssh ─────────────────────────────────────────────────────────────

#[test]
fn entry_keygen_with_comment_parses() {
    let cli = parse(&["entry", "keygen", "alice@laptop"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Keygen {
            comment: Some(ref c),
            name: None,
            vault: None,
        })) if c == "alice@laptop"
    ));
}

#[test]
fn entry_keygen_without_comment_parses() {
    let cli = parse(&["entry", "keygen"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Keygen {
            comment: None,
            name: None,
            vault: None,
        }))
    ));
}

#[test]
fn entry_keygen_with_name_and_vault_parses() {
    let cli = parse(&[
        "entry",
        "keygen",
        "alice@laptop",
        "--name",
        "work-key",
        "--vault",
        "work",
    ]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Keygen {
            comment: Some(ref c),
            name: Some(ref n),
            vault: Some(ref v),
        })) if c == "alice@laptop" && n == "work-key" && v == "work"
    ));
}

#[test]
fn entry_rotate_parses() {
    let cli = parse(&["entry", "rotate", "work-key"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Rotate {
            ref name,
            comment: None,
            vault: None,
        })) if name == "work-key"
    ));
}

#[test]
fn entry_rotate_with_comment_parses() {
    let cli = parse(&["entry", "rotate", "work-key", "--comment", "alice@newhost"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Rotate {
            ref name,
            comment: Some(ref c),
            vault: None,
        })) if name == "work-key" && c == "alice@newhost"
    ));
}

#[test]
fn entry_pubkey_parses() {
    let cli = parse(&["entry", "pubkey", "work-key"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Pubkey {
            ref name,
            copy: false,
            vault: None,
        })) if name == "work-key"
    ));
}

#[test]
fn entry_pubkey_with_copy_parses() {
    let cli = parse(&["entry", "pubkey", "work-key", "--copy"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Pubkey {
            ref name,
            copy: true,
            vault: None,
        })) if name == "work-key"
    ));
}

#[test]
fn entry_serve_on_parses() {
    let cli = parse(&["entry", "serve", "work-key", "--on"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Serve {
            ref name,
            on: true,
            off: false,
            vault: None,
        })) if name == "work-key"
    ));
}

#[test]
fn entry_serve_off_parses() {
    let cli = parse(&["entry", "serve", "work-key", "--off"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Entry(EntryCommands::Serve {
            ref name,
            on: false,
            off: true,
            vault: None,
        })) if name == "work-key"
    ));
}

#[test]
fn entry_serve_without_state_is_rejected() {
    let result = Cli::try_parse_from(["bogita", "entry", "serve", "work-key"]);
    assert!(result.is_err(), "--on or --off is required");
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
    let cli = parse(&["vault", "lock"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Vault(VaultCommands::Lock))
    ));
}

#[test]
fn vault_unlock_parses() {
    let cli = parse(&["vault", "unlock"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Vault(VaultCommands::Unlock {
            password: None,
            password_stdin: false,
            password_file: None,
        }))
    ));
}

#[test]
fn vault_unlock_with_password_parses() {
    let cli = parse(&["vault", "unlock", "--password", "correct horse"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Vault(VaultCommands::Unlock {
            password: Some(ref value),
            password_stdin: false,
            password_file: None,
        })) if value == "correct horse"
    ));
}

#[test]
fn vault_unlock_short_password_flag_parses() {
    let cli = parse(&["vault", "unlock", "-p", "correct horse"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Vault(VaultCommands::Unlock {
            password: Some(ref value),
            ..
        })) if value == "correct horse"
    ));
}

#[test]
fn vault_unlock_with_password_stdin_parses() {
    let cli = parse(&["vault", "unlock", "--password-stdin"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Vault(VaultCommands::Unlock {
            password: None,
            password_stdin: true,
            password_file: None,
        }))
    ));
}

#[test]
fn vault_unlock_with_password_file_parses() {
    let cli = parse(&["vault", "unlock", "--password-file", "/tmp/pass"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Vault(VaultCommands::Unlock {
            password: None,
            password_stdin: false,
            password_file: Some(ref path),
        })) if path == &std::path::PathBuf::from("/tmp/pass")
    ));
}

#[test]
fn vault_unlock_accepts_one_source_at_a_time() {
    let cases: [&[&str]; 3] = [
        &["vault", "unlock", "--password", "x", "--password-stdin"],
        &[
            "vault",
            "unlock",
            "--password",
            "x",
            "--password-file",
            "/tmp/p",
        ],
        &[
            "vault",
            "unlock",
            "--password-stdin",
            "--password-file",
            "/tmp/p",
        ],
    ];
    for args in cases {
        // -- Exec
        let result = Cli::try_parse_from(args);

        // -- Check: clap rejects the combination with its usage-error exit code
        let error = result.expect_err("two passphrase sources must be rejected");
        assert_eq!(error.exit_code(), 2, "args: {args:?}");
    }
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

#[test]
fn vault_rm_parses() {
    let cli = parse(&["vault", "rm", "personal"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Vault(VaultCommands::Rm { ref name })) if name == "personal"
    ));
}

// ── agent ─────────────────────────────────────────────────────────────────

#[test]
fn agent_start_parses() {
    let cli = parse(&["agent", "start"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Agent(AgentCommands::Start {
            socket: None,
            internal: false,
        }))
    ));
}

#[test]
fn agent_start_with_socket_parses() {
    let cli = parse(&["agent", "start", "--socket", "/tmp/agent.sock"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Agent(AgentCommands::Start {
            socket: Some(ref s),
            internal: false,
        })) if s == "/tmp/agent.sock"
    ));
}

#[test]
fn agent_start_internal_parses() {
    let cli = parse(&["agent", "start", "--internal"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Agent(AgentCommands::Start {
            socket: None,
            internal: true,
        }))
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

// ── hidden git helpers ──────────────────────────────────────────────────────

#[test]
fn ssh_sign_helper_parses_git_argv_verbatim() {
    let cli = parse(&[
        "ssh-sign-helper",
        "-Y",
        "sign",
        "-n",
        "git",
        "-f",
        "/tmp/key",
        "-U",
        "/tmp/data",
    ]);
    let expected = vec![
        "-Y",
        "sign",
        "-n",
        "git",
        "-f",
        "/tmp/key",
        "-U",
        "/tmp/data",
    ];
    assert!(matches!(
        cli.command,
        Some(Commands::SshSignHelper { ref args }) if args == &expected
    ));
}

#[test]
fn ssh_default_key_parses() {
    let cli = parse(&["ssh-default-key"]);
    assert!(matches!(cli.command, Some(Commands::SshDefaultKey)));
}

#[test]
fn hidden_helpers_do_not_appear_in_help() {
    // -- Setup & Fixtures
    let mut command = Cli::command();

    // -- Exec
    let help = command.render_help().to_string();

    // -- Check
    assert!(!help.contains("ssh-sign-helper"), "help: {help}");
    assert!(!help.contains("ssh-default-key"), "help: {help}");
    assert!(help.contains("entry"), "the visible commands stay listed");
}
