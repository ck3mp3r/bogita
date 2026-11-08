use bogita_cli::args::{Cli, CliCommand, Commands, EntryCommands, VaultCommands};
use bogita_cli::handlers::entry::{handle_get, handle_ls, handle_search, EntryOutput, GetOutput};
use bogita_cli::handlers::vault::{handle_default, handle_list, handle_rm};
use bogita_core::app::{App, DefaultApp, InitParts, InitResult};
use bogita_core::domain::{FieldType, FieldValue, SecretString};
use bogita_core::ports::{IdentityStore, KeychainStore, VaultStorageFactory};
use bogita_tui::app::Tui;
use bogita_tui::context::TuiContext;
use clap::Parser;

/// Prompt used whenever an existing identity must be decrypted.
const UNLOCK_PROMPT: &str = "Enter passphrase: ";
/// Prompt used on first run, when the identity is created.
const FIRST_RUN_PROMPT: &str = "Set a passphrase: ";

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        None => launch_tui(TuiContext::Default).await,
        Some(cmd) => match dispatch(cmd) {
            Dispatch::Tui(ctx) => launch_tui(ctx).await,
            Dispatch::Lock => lock_vault().await,
            Dispatch::Unlock => unlock_vault().await,
            Dispatch::Cli(cli_cmd) => run_cli(cli_cmd, init_app().await).await,
            Dispatch::Stub => not_implemented(),
        },
    }
}

/// What the binary does with a parsed command.
enum Dispatch {
    /// Launch the TUI focused on the given view.
    Tui(TuiContext),
    /// Lock the vault (remove the identity from the keychain).
    Lock,
    /// Unlock the vault (prompt for the passphrase).
    Unlock,
    /// Run a command against an initialised app.
    Cli(CliCommand),
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
        Commands::Vault(VaultCommands::Add { name }) => {
            Dispatch::Tui(TuiContext::AddVault { name })
        }
        Commands::Vault(VaultCommands::Lock) => Dispatch::Lock,
        Commands::Vault(VaultCommands::Unlock) => Dispatch::Unlock,
        Commands::Vault(VaultCommands::List) => Dispatch::Cli(CliCommand::VaultList),
        Commands::Vault(VaultCommands::Default { name }) => {
            Dispatch::Cli(CliCommand::VaultDefault { name })
        }
        Commands::Vault(VaultCommands::Rm { name }) => Dispatch::Cli(CliCommand::VaultRm { name }),
        Commands::Vault(VaultCommands::Sync { .. }) => Dispatch::Stub,
        Commands::Agent(_) => Dispatch::Stub,
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

/// Unlock the vault (prompt for the passphrase).
async fn unlock_vault() {
    match DefaultApp::init().await {
        InitResult::Ready(_) => {
            println!("vault is already unlocked");
        }
        InitResult::Locked(parts) => {
            finish_init(parts, false)
                .await
                .unwrap_or_else(|message| exit_with_error(&message));
            println!("vault unlocked");
        }
        InitResult::NeedsPassphrase(parts) => {
            finish_init(parts, true)
                .await
                .unwrap_or_else(|message| exit_with_error(&message));
            println!("vault unlocked");
        }
        InitResult::Failed { message } => exit_with_error(&message),
    }
}

/// Prompt for the passphrase a pending init needs and finish it.
async fn finish_init<K, F, I>(
    parts: InitParts<K, F, I>,
    first_run: bool,
) -> Result<App<K, F, I>, String>
where
    K: KeychainStore,
    F: VaultStorageFactory,
    I: IdentityStore,
{
    if first_run {
        let passphrase = prompt_passphrase(FIRST_RUN_PROMPT);
        App::complete_first_run(parts, &passphrase)
            .await
            .map_err(|e| e.to_string())
    } else {
        let passphrase = prompt_passphrase(UNLOCK_PROMPT);
        App::complete_unlock(parts, &passphrase)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Report a fatal init error and stop.
fn exit_with_error(message: &str) -> ! {
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
            finish_init(parts, true)
                .await
                .unwrap_or_else(|message| exit_with_error(&message))
        }
        InitResult::Locked(parts) => {
            eprintln!("Vault is locked. Enter your passphrase to unlock.");
            finish_init(parts, false)
                .await
                .unwrap_or_else(|message| exit_with_error(&message))
        }
        InitResult::Failed { message } => exit_with_error(&message),
    }
}

/// Prompt for a passphrase (simple stdin read).
fn prompt_passphrase(prompt: &str) -> SecretString {
    use std::io::Write;
    print!("{prompt}");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).ok();
    SecretString::from(line.trim().to_string())
}

/// Returns `true` if a command requires the vault to be unlocked (identity loaded).
fn command_needs_identity(cmd: &CliCommand) -> bool {
    matches!(
        cmd,
        CliCommand::EntryLs { .. }
            | CliCommand::EntryGet { .. }
            | CliCommand::EntrySearch { .. }
            | CliCommand::EntryCp { .. }
    )
}

/// Report a subcommand that has no implementation yet and stop with exit 2.
fn not_implemented() -> ! {
    eprintln!("not yet implemented");
    std::process::exit(2);
}

/// Context passed to the TUI on startup to pre-focus the right view.
async fn launch_tui(ctx: TuiContext) {
    let app = match DefaultApp::init().await {
        InitResult::Ready(app) => app,
        InitResult::NeedsPassphrase(parts) => finish_init(parts, true)
            .await
            .unwrap_or_else(|message| exit_with_error(&message)),
        InitResult::Locked(parts) => finish_init(parts, false)
            .await
            .unwrap_or_else(|message| exit_with_error(&message)),
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

async fn run_cli(cmd: CliCommand, app: DefaultApp) {
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
                Ok(GetOutput::Field(value)) => {
                    use bogita_core::domain::SecretString;
                    use bogita_core::service::clipboard::{system, CLIPBOARD_TIMEOUT_SECS};
                    let svc = system();
                    match svc
                        .copy_with_timeout(SecretString::from(value), CLIPBOARD_TIMEOUT_SECS)
                        .await
                    {
                        Ok(()) => {
                            eprintln!("copied to clipboard (clears in {CLIPBOARD_TIMEOUT_SECS}s)")
                        }
                        Err(e) => eprintln!("clipboard error: {e}"),
                    }
                }
                Ok(GetOutput::Entry(_)) => eprintln!("error: field value not resolved"),
                Err(e) => eprintln!("error: {e}"),
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
