use bogita_cli::args::{AgentCommands, Cli, Commands, EntryCommands, VaultCommands};
use bogita_cli::handlers::entry::{handle_get, handle_ls, handle_search, EntryOutput};
use bogita_cli::handlers::vault::{handle_vault, VaultOutput};
use bogita_core::app::App;
use bogita_core::domain::{FieldType, FieldValue};
use bogita_tui::app::Tui;
use bogita_tui::context::TuiContext;
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
                let output =
                    handle_ls(EntryCommands::Ls { vault }, app.registry, app.identity).await;
                match output {
                    Ok(EntryOutput::List(entries)) => {
                        for entry in entries {
                            println!("{}", entry.name);
                        }
                    }
                    Ok(_) => unreachable!("Ls should only return List"),
                    Err(e) => eprintln!("error: {e}"),
                }
            }
            EntryCommands::Get { name, field, vault } => {
                let output = handle_get(
                    EntryCommands::Get { name, field, vault },
                    app.registry,
                    app.identity,
                )
                .await;
                match output {
                    Ok(EntryOutput::Entry(entry)) => {
                        println!("name: {}", entry.name);
                        println!("type: {:?}", entry.entry_type);
                        for field in &entry.fields {
                            let value = if field.field_type == FieldType::TotpSecret
                                || matches!(field.value, FieldValue::Hidden(_))
                            {
                                "****".to_string()
                            } else {
                                format!("{:?}", field.value)
                            };
                            println!("  {}: {}", field.key, value);
                        }
                    }
                    Ok(EntryOutput::Field(value)) => println!("{}", value),
                    Ok(_) => unreachable!("Get should only return Entry or Field"),
                    Err(e) => eprintln!("error: {e}"),
                }
            }
            EntryCommands::Search { query, vault } => {
                let output = handle_search(
                    EntryCommands::Search { query, vault },
                    app.registry,
                    app.identity,
                )
                .await;
                match output {
                    Ok(EntryOutput::List(entries)) => {
                        for entry in entries {
                            println!("{}", entry.name);
                        }
                    }
                    Ok(_) => unreachable!("Search should only return List"),
                    Err(e) => eprintln!("error: {e}"),
                }
            }
            // TUI mutations are handled before run_cli is called
            EntryCommands::Add { .. } | EntryCommands::Edit { .. } | EntryCommands::Rm { .. } => {
                unreachable!("entry mutations handled as TUI deep-links")
            }
            EntryCommands::Cp { name, field, vault } => {
                let output = handle_get(
                    EntryCommands::Get {
                        name: name.clone(),
                        field: Some(field.clone()),
                        vault: vault.clone(),
                    },
                    app.registry,
                    app.identity,
                )
                .await;
                match output {
                    Ok(EntryOutput::Field(value)) => {
                        use bogita_core::domain::SecretString;
                        use bogita_core::service::clipboard::{ArboardBackend, ClipboardService};
                        let svc = ClipboardService::new(ArboardBackend);
                        match svc.copy_with_timeout(SecretString::from(value), 30).await {
                            Ok(()) => eprintln!("copied to clipboard (clears in 30s)"),
                            Err(e) => eprintln!("clipboard error: {e}"),
                        }
                    }
                    Ok(_) => eprintln!("unexpected response"),
                    Err(e) => eprintln!("error: {e}"),
                }
            }
        },
        Commands::Vault(vault_cmd) => match vault_cmd {
            VaultCommands::List => {
                let output = handle_vault(VaultCommands::List, app.registry).await;
                match output {
                    Ok(VaultOutput::List(vaults)) => {
                        for vault in vaults {
                            let marker = if vault.is_default { "*" } else { " " };
                            println!("{} {}", marker, vault.name);
                        }
                    }
                    Ok(_) => unreachable!("List should only return List"),
                    Err(e) => eprintln!("error: {e}"),
                }
            }
            VaultCommands::Default { name } => {
                let output = handle_vault(VaultCommands::Default { name }, app.registry).await;
                match output {
                    Ok(VaultOutput::Ok) => println!("default vault set"),
                    Ok(_) => unreachable!("Default should only return Ok"),
                    Err(e) => eprintln!("error: {e}"),
                }
            }
            VaultCommands::Lock { .. }
            | VaultCommands::Unlock { .. }
            | VaultCommands::Sync { .. } => {
                eprintln!("not yet implemented");
            }
            VaultCommands::Add { .. } => unreachable!("vault add handled as TUI deep-link"),
            VaultCommands::Rm { name } => {
                let output =
                    handle_vault(VaultCommands::Rm { name: name.clone() }, app.registry).await;
                match output {
                    Ok(VaultOutput::Ok) => println!("vault '{}' removed", name),
                    Ok(_) => unreachable!("Rm should only return Ok"),
                    Err(e) => eprintln!("error: {e}"),
                }
            }
        },
        Commands::Agent(agent_cmd) => match agent_cmd {
            AgentCommands::Start { socket } => {
                let _ = (socket, app);
                eprintln!("not yet implemented");
            }
            AgentCommands::Stop => eprintln!("not yet implemented"),
            AgentCommands::Status => eprintln!("not yet implemented"),
            AgentCommands::Keys => eprintln!("not yet implemented"),
        },
    }
}
