use bogita::app::App;
use bogita::cli::args::{AgentCommands, Cli, Commands, EntryCommands, VaultCommands};
use bogita::tui::app::Tui;
use bogita::tui::context::TuiContext;
use clap::Parser;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        None => {
            // No args — launch TUI (default)
            launch_tui(TuiContext::Default).await;
        }

        // ── Entry TUI mutations ────────────────────────────────────────────
        Some(Commands::Entry(EntryCommands::Add { name, vault })) => {
            launch_tui(TuiContext::AddEntry { name, vault }).await;
        }
        Some(Commands::Entry(EntryCommands::Edit { name, vault })) => {
            launch_tui(TuiContext::EditEntry { name, vault }).await;
        }
        Some(Commands::Entry(EntryCommands::Rm { name, vault })) => {
            launch_tui(TuiContext::DeleteEntry { name, vault }).await;
        }

        // ── Vault TUI mutation ─────────────────────────────────────────────
        Some(Commands::Vault(VaultCommands::Add { name })) => {
            launch_tui(TuiContext::AddVault { name }).await;
        }

        // ── Read-only / stateless CLI ──────────────────────────────────────
        Some(cmd) => {
            let app = App::init().await.unwrap_or_else(|e| {
                eprintln!("error: {e}");
                std::process::exit(1);
            });
            run_cli(cmd, app).await;
        }
    }
}

/// Context passed to the TUI on startup to pre-focus the right view.
async fn launch_tui(ctx: TuiContext) {
    let app = App::init().await.unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });
    let tui = Tui::new(app, ctx).await.unwrap_or_else(|e| {
        eprintln!("error initialising tui: {e}");
        std::process::exit(1);
    });
    if let Err(e) = tui.run().await {
        eprintln!("tui error: {e}");
        std::process::exit(1);
    }
}

async fn run_cli(cmd: Commands, app: App) {
    match cmd {
        Commands::Entry(entry_cmd) => match entry_cmd {
            EntryCommands::Ls { vault } => {
                let _ = (vault, app);
                eprintln!("entry ls not yet implemented");
            }
            EntryCommands::Get { name, field, vault } => {
                let _ = (name, field, vault, app);
                eprintln!("entry get not yet implemented");
            }
            EntryCommands::Search { query, vault } => {
                let _ = (query, vault, app);
                eprintln!("entry search not yet implemented");
            }
            // TUI mutations are handled before run_cli is called
            EntryCommands::Add { .. } | EntryCommands::Edit { .. } | EntryCommands::Rm { .. } => {
                unreachable!("entry mutations handled as TUI deep-links")
            }
        },
        Commands::Vault(vault_cmd) => match vault_cmd {
            VaultCommands::List => {
                let _ = app;
                eprintln!("vault list not yet implemented");
            }
            VaultCommands::Lock { name } => {
                let _ = (name, app);
                eprintln!("vault lock not yet implemented");
            }
            VaultCommands::Unlock { name } => {
                let _ = (name, app);
                eprintln!("vault unlock not yet implemented");
            }
            VaultCommands::Sync { name } => {
                let _ = (name, app);
                eprintln!("vault sync not yet implemented");
            }
            VaultCommands::Default { name } => {
                let _ = (name, app);
                eprintln!("vault default not yet implemented");
            }
            VaultCommands::Add { .. } => unreachable!("vault add handled as TUI deep-link"),
        },
        Commands::Agent(agent_cmd) => match agent_cmd {
            AgentCommands::Start { socket } => {
                let _ = (socket, app);
                eprintln!("agent start not yet implemented");
            }
            AgentCommands::Stop => eprintln!("agent stop not yet implemented"),
            AgentCommands::Status => eprintln!("agent status not yet implemented"),
            AgentCommands::Keys => eprintln!("agent keys not yet implemented"),
        },
    }
}
