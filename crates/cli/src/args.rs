//! Clap-derived CLI argument definitions.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "bogita",
    version,
    about = "Password manager with SSH agent integration"
)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Manage entries (add, edit, rm, get, ls, search)
    #[command(subcommand)]
    Entry(EntryCommands),

    /// Vault management
    #[command(subcommand)]
    Vault(VaultCommands),

    /// SSH agent
    #[command(subcommand)]
    Agent(AgentCommands),
}

#[derive(Subcommand, Debug)]
pub enum EntryCommands {
    // ── Read-only ──────────────────────────────────────────────────────────
    /// List all entries
    Ls {
        #[arg(long, short = 'v')]
        vault: Option<String>,
    },

    /// Get an entry or a specific field value
    ///
    /// Field-type-aware: TotpSecret fields compute and return the live TOTP code.
    Get {
        name: String,
        /// Field key to retrieve. Omit to print all fields (Hidden values masked).
        #[arg(long, short)]
        field: Option<String>,
        #[arg(long, short = 'v')]
        vault: Option<String>,
    },

    /// Search entries by name or field value
    Search {
        query: String,
        #[arg(long, short = 'v')]
        vault: Option<String>,
    },

    /// Copy a field value to clipboard (auto-clears after 30s)
    Cp {
        name: String,
        #[arg(long, short)]
        field: String,
        #[arg(long, short = 'v')]
        vault: Option<String>,
    },

    // ── TUI mutations ──────────────────────────────────────────────────────
    /// Add a new entry (opens TUI)
    Add {
        name: Option<String>,
        #[arg(long, short = 'v')]
        vault: Option<String>,
    },

    /// Edit an entry (opens TUI)
    Edit {
        name: String,
        #[arg(long, short = 'v')]
        vault: Option<String>,
    },

    /// Delete an entry (opens TUI)
    Rm {
        name: String,
        #[arg(long, short = 'v')]
        vault: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum VaultCommands {
    /// List all vaults
    List,
    /// Lock a vault
    Lock { name: String },
    /// Unlock a vault
    Unlock { name: String },
    /// Sync a vault with its git backend
    Sync { name: Option<String> },
    /// Set the default vault
    Default { name: String },
    /// Remove a vault
    Rm { name: String },
    /// Create a new vault (opens TUI)
    Add { name: String },
}

#[derive(Subcommand, Debug)]
pub enum AgentCommands {
    /// Start the SSH agent daemon
    Start {
        #[arg(long)]
        socket: Option<String>,
    },
    /// Stop the SSH agent
    Stop,
    /// Show agent status
    Status,
    /// List keys served by the agent
    Keys,
}
