use bogita_cli::args::{AgentCommands, Cli, CliCommand, Commands, EntryCommands, VaultCommands};
use bogita_core::app::{App, DefaultApp, InitParts, InitResult};
use bogita_core::domain::SecretString;
use bogita_core::ports::{IdentityStore, KeychainStore, VaultStorageFactory};
use bogita_tui::app::Tui;
use bogita_tui::context::TuiContext;
use clap::Parser;
use std::ffi::OsString;
use zeroize::Zeroize;

// region:    --- Modules

mod agent;
mod cli;
mod ssh_helper;
mod unlock;

use crate::agent::{
    run_agent_daemon, run_agent_keys, run_agent_start, run_agent_status, run_agent_stop,
};
use crate::cli::run_cli;
use crate::ssh_helper::{run_ssh_default_key, run_ssh_sign_helper};
use crate::unlock::{resolve_unlock_secret, UnlockSecret};

// endregion: --- Modules

/// Prompt used whenever an existing identity must be decrypted.
const UNLOCK_PROMPT: &str = "Enter passphrase: ";
/// Prompt used on first run, when the identity is created.
const FIRST_RUN_PROMPT: &str = "Set a passphrase: ";

#[tokio::main]
async fn main() {
    // git runs `gpg.ssh.program` directly, so the program can be this binary and
    // git's own `-Y` arrives as the first argument. Answer it before clap sees it;
    // the hidden `ssh-sign-helper` subcommand serves wrapper scripts instead.
    if let Some(args) = git_helper_args() {
        run_ssh_sign_helper(args).await;
        return;
    }

    let cli = Cli::parse();

    match cli.command {
        None => launch_tui(TuiContext::Default).await,
        Some(cmd) => match dispatch(cmd) {
            Dispatch::Tui(ctx) => launch_tui(ctx).await,
            Dispatch::Lock => lock_vault().await,
            Dispatch::Unlock(secret) => unlock_vault(secret).await,
            Dispatch::Cli(cli_cmd) => run_cli(cli_cmd, init_app().await).await,
            Dispatch::AgentStart { socket } => run_agent_start(socket).await,
            Dispatch::AgentServe { socket } => run_agent_daemon(socket).await,
            Dispatch::AgentStop => run_agent_stop(),
            Dispatch::AgentStatus => run_agent_status(),
            Dispatch::AgentKeys => run_agent_keys(),
            Dispatch::SshSignHelper(args) => run_ssh_sign_helper(args).await,
            Dispatch::SshDefaultKey => run_ssh_default_key().await,
            Dispatch::Stub => not_implemented(),
        },
    }
}

/// Raw git helper argv, when the binary itself is the configured
/// `gpg.ssh.program`.
///
/// git passes the full `-Y ...` command line to the program; clap would reject
/// it, so the guard reads the raw arguments first.
fn git_helper_args() -> Option<Vec<String>> {
    let raw: Vec<OsString> = std::env::args_os().skip(1).collect();
    let first = raw.first()?;
    if first != "-Y" {
        return None;
    }
    Some(
        raw.iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect(),
    )
}

/// What the binary does with a parsed command.
enum Dispatch {
    /// Launch the TUI focused on the given view.
    Tui(TuiContext),
    /// Lock the vault (remove the identity from the keychain).
    Lock,
    /// Unlock the vault with the resolved passphrase source.
    Unlock(UnlockSecret),
    /// Run a command against an initialised app.
    Cli(CliCommand),
    /// Spawn the agent daemon and report its socket.
    AgentStart { socket: Option<String> },
    /// Serve the agent in this process — the hidden `--internal` child.
    AgentServe { socket: Option<String> },
    /// Stop the agent daemon.
    AgentStop,
    /// Report agent state.
    AgentStatus,
    /// List the keys the agent serves.
    AgentKeys,
    /// Answer one git `gpg.ssh.program` invocation.
    SshSignHelper(Vec<String>),
    /// Print the public key git should sign with by default.
    SshDefaultKey,
    /// A subcommand with no implementation yet — reported before any init work.
    Stub,
}

/// Map every parsed command to exactly one action.
///
/// The variant is known in each arm, so no command path needs a guard that
/// cannot be reached.
fn dispatch(cmd: Commands) -> Dispatch {
    match cmd {
        Commands::Entry(EntryCommands::Add { name, vault }) => {
            Dispatch::Tui(TuiContext::AddEntry { name, vault })
        }
        Commands::Entry(EntryCommands::Edit { name, vault }) => {
            Dispatch::Tui(TuiContext::EditEntry { name, vault })
        }
        Commands::Entry(EntryCommands::Rm { name, vault }) => {
            Dispatch::Tui(TuiContext::DeleteEntry { name, vault })
        }
        Commands::Entry(EntryCommands::Ls { vault }) => {
            Dispatch::Cli(CliCommand::EntryLs { vault })
        }
        Commands::Entry(EntryCommands::Get { name, field, vault }) => {
            Dispatch::Cli(CliCommand::EntryGet { name, field, vault })
        }
        Commands::Entry(EntryCommands::Search { query, vault }) => {
            Dispatch::Cli(CliCommand::EntrySearch { query, vault })
        }
        Commands::Entry(EntryCommands::Cp { name, field, vault }) => {
            Dispatch::Cli(CliCommand::EntryCp { name, field, vault })
        }
        Commands::Entry(EntryCommands::Keygen {
            comment,
            name,
            vault,
        }) => Dispatch::Cli(CliCommand::EntryKeygen {
            comment,
            name,
            vault,
        }),
        Commands::Entry(EntryCommands::Rotate {
            name,
            comment,
            vault,
        }) => Dispatch::Cli(CliCommand::EntryRotate {
            name,
            comment,
            vault,
        }),
        Commands::Entry(EntryCommands::Pubkey { name, copy, vault }) => {
            Dispatch::Cli(CliCommand::EntryPubkey { name, copy, vault })
        }
        Commands::Entry(EntryCommands::Serve {
            name,
            on,
            off,
            vault,
        }) => Dispatch::Cli(CliCommand::EntryServe {
            name,
            on: on && !off,
            vault,
        }),
        Commands::Vault(VaultCommands::Add { name }) => {
            Dispatch::Tui(TuiContext::AddVault { name })
        }
        Commands::Vault(VaultCommands::Lock) => Dispatch::Lock,
        Commands::Vault(VaultCommands::Unlock {
            password,
            password_stdin,
            password_file,
        }) => Dispatch::Unlock(
            resolve_unlock_secret(
                password,
                password_stdin,
                password_file,
                &mut std::io::stdin().lock(),
            )
            .unwrap_or_else(|error| error.report()),
        ),
        Commands::Vault(VaultCommands::List) => Dispatch::Cli(CliCommand::VaultList),
        Commands::Vault(VaultCommands::Default { name }) => {
            Dispatch::Cli(CliCommand::VaultDefault { name })
        }
        Commands::Vault(VaultCommands::Rm { name }) => Dispatch::Cli(CliCommand::VaultRm { name }),
        Commands::Vault(VaultCommands::Sync { .. }) => Dispatch::Stub,
        Commands::Agent(AgentCommands::Start { socket, internal }) => {
            if internal {
                Dispatch::AgentServe { socket }
            } else {
                Dispatch::AgentStart { socket }
            }
        }
        Commands::Agent(AgentCommands::Stop) => Dispatch::AgentStop,
        Commands::Agent(AgentCommands::Status) => Dispatch::AgentStatus,
        Commands::Agent(AgentCommands::Keys) => Dispatch::AgentKeys,
        Commands::SshSignHelper { args } => Dispatch::SshSignHelper(args),
        Commands::SshDefaultKey => Dispatch::SshDefaultKey,
    }
}

/// Lock the vault (remove the identity from the keychain).
async fn lock_vault() {
    match DefaultApp::init().await {
        InitResult::Ready(mut app) => {
            app.lock().unwrap_or_else(|e| {
                eprintln!("error: {e}");
                std::process::exit(1);
            });
            println!("vault locked");
        }
        InitResult::Locked(_) => {
            println!("vault is already locked");
        }
        InitResult::Failed { message } => {
            eprintln!("error: {message}");
            std::process::exit(1);
        }
        InitResult::NeedsPassphrase(_) => {
            println!("vault is not initialized");
        }
    }
}

/// Unlock the vault with the resolved passphrase source.
async fn unlock_vault(secret: UnlockSecret) {
    let passphrase = secret.into_passphrase();
    match DefaultApp::init().await {
        InitResult::Ready(_) => {
            println!("vault is already unlocked");
        }
        InitResult::Locked(parts) => {
            finish_init(parts, false, passphrase)
                .await
                .unwrap_or_else(|message| exit_with_error(&message));
            println!("vault unlocked");
        }
        InitResult::NeedsPassphrase(parts) => {
            finish_init(parts, true, passphrase)
                .await
                .unwrap_or_else(|message| exit_with_error(&message));
            println!("vault unlocked");
        }
        InitResult::Failed { message } => exit_with_error(&message),
    }
}

/// Prompt for the passphrase a pending init needs and finish it.
///
/// `passphrase` carries a resolved automation source: `Some` skips the terminal
/// prompt entirely and uses the secret unchanged.
async fn finish_init<K, F, I>(
    parts: InitParts<K, F, I>,
    first_run: bool,
    passphrase: Option<SecretString>,
) -> Result<App<K, F, I>, String>
where
    K: KeychainStore,
    F: VaultStorageFactory,
    I: IdentityStore,
{
    if first_run {
        let passphrase = passphrase.unwrap_or_else(|| prompt_passphrase(FIRST_RUN_PROMPT));
        App::complete_first_run(parts, &passphrase)
            .await
            .map_err(|e| e.to_string())
    } else {
        let passphrase = passphrase.unwrap_or_else(|| prompt_passphrase(UNLOCK_PROMPT));
        App::complete_unlock(parts, &passphrase)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Report a fatal init error and stop.
pub(crate) fn exit_with_error(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1);
}

/// Initialize the app, handling all init paths.
/// For CLI mode, we prompt for passphrase if needed.
async fn init_app() -> DefaultApp {
    match DefaultApp::init().await {
        InitResult::Ready(app) => app,
        InitResult::NeedsPassphrase(parts) => {
            eprintln!("First run detected. Please set a passphrase to encrypt your identity.");
            finish_init(parts, true, None)
                .await
                .unwrap_or_else(|message| exit_with_error(&message))
        }
        InitResult::Locked(parts) => {
            eprintln!("Vault is locked. Enter your passphrase to unlock.");
            finish_init(parts, false, None)
                .await
                .unwrap_or_else(|message| exit_with_error(&message))
        }
        InitResult::Failed { message } => exit_with_error(&message),
    }
}

/// Prompt for a passphrase on the terminal; echo stays off while typing.
fn prompt_passphrase(prompt: &str) -> SecretString {
    match rpassword::prompt_password(prompt) {
        Ok(raw) => secretize(raw),
        Err(_) => SecretString::from(String::new()),
    }
}

/// Move a raw passphrase into a secret and zeroize the raw copy.
pub(crate) fn secretize(mut raw: String) -> SecretString {
    let trimmed = raw.trim().to_owned();
    raw.zeroize();
    SecretString::from(trimmed)
}

/// Report a subcommand that has no implementation yet and stop with exit 2.
fn not_implemented() -> ! {
    eprintln!("not yet implemented");
    std::process::exit(2);
}

/// Context passed to the TUI on startup to pre-focus the right view.
///
/// A locked vault boots as a locked app: the TUI's own lock screen asks for the
/// passphrase, so no terminal prompt runs on this path. First run still prompts
/// here — it creates the identity, which the TUI cannot do yet.
async fn launch_tui(ctx: TuiContext) {
    let app = match DefaultApp::init().await {
        InitResult::Ready(app) => app,
        InitResult::NeedsPassphrase(parts) => finish_init(parts, true, None)
            .await
            .unwrap_or_else(|message| exit_with_error(&message)),
        InitResult::Locked(parts) => App::pending(parts),
        InitResult::Failed { message } => exit_with_error(&message),
    };
    let tui = Tui::new(app, ctx).await.unwrap_or_else(|e| {
        eprintln!("error initialising tui: {e}");
        std::process::exit(1);
    });
    if let Err(e) = tui.run().await {
        eprintln!("tui error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
#[path = "../test/secret_prompt.rs"]
mod secret_prompt_test;
