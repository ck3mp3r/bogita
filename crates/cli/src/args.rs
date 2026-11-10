//! Clap-derived CLI argument definitions.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

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

    /// Git SSH sign helper — invoked by git through gpg.ssh.program
    #[command(hide = true, name = "ssh-sign-helper")]
    SshSignHelper {
        /// Arguments git passed to gpg.ssh.program, verbatim
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 0..)]
        args: Vec<String>,
    },

    /// Git SSH default-key helper — gpg.ssh.defaultKeyCommand
    #[command(hide = true, name = "ssh-default-key")]
    SshDefaultKey,
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

    // ── SSH keys ───────────────────────────────────────────────────────────
    /// Generate an Ed25519 SSH key entry
    Keygen {
        /// Key comment (defaults to the entry name)
        comment: Option<String>,
        /// Entry name (defaults to the comment, then "ssh-key")
        #[arg(long, short = 'n')]
        name: Option<String>,
        #[arg(long, short = 'v')]
        vault: Option<String>,
    },

    /// Replace the key pair of an SSH key entry
    Rotate {
        name: String,
        /// New key comment (defaults to the stored comment)
        #[arg(long, short)]
        comment: Option<String>,
        #[arg(long, short = 'v')]
        vault: Option<String>,
    },

    /// Print an SSH entry's public key line
    Pubkey {
        name: String,
        /// Copy the public key to the clipboard (auto-clears after 30s)
        #[arg(long, short)]
        copy: bool,
        #[arg(long, short = 'v')]
        vault: Option<String>,
    },

    /// Toggle whether the agent serves an SSH key
    #[command(group(
        clap::ArgGroup::new("serve_state")
            .args(["on", "off"])
            .required(true)
            .multiple(false)
    ))]
    Serve {
        name: String,
        /// Mark the key as servable by the agent
        #[arg(long)]
        on: bool,
        /// Stop serving the key
        #[arg(long)]
        off: bool,
        #[arg(long, short = 'v')]
        vault: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum VaultCommands {
    /// List all vaults
    List,
    /// Lock the vault (remove identity from keychain)
    Lock,
    /// Unlock the vault (prompt for passphrase)
    ///
    /// Without a passphrase flag the prompt runs on the terminal with echo off.
    /// A flag source never prompts: the passphrase comes from that source and
    /// errors go to stderr with a non-zero exit code.
    ///
    /// Every source trims surrounding whitespace, exactly as the interactive
    /// prompt trims the typed passphrase.
    Unlock {
        /// Passphrase, taken from the command line
        ///
        /// The value is visible in shell history and process listings (`ps`).
        /// Prefer `--password-stdin` or `--password-file`.
        #[arg(
            long,
            short = 'p',
            value_name = "PASSPHRASE",
            conflicts_with_all = ["password_stdin", "password_file"]
        )]
        password: Option<String>,
        /// Read the passphrase from one line on stdin
        ///
        /// The trailing newline is stripped; nothing is echoed.
        #[arg(long, conflicts_with_all = ["password", "password_file"])]
        password_stdin: bool,
        /// Read the passphrase from a file
        ///
        /// One trailing newline is stripped. The file must not be group- or
        /// other-readable, so a shared file cannot leak the passphrase:
        /// `chmod 600` it first.
        #[arg(
            long,
            value_name = "PATH",
            conflicts_with_all = ["password", "password_stdin"]
        )]
        password_file: Option<PathBuf>,
    },
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
    /// Start the agent daemon and print its socket as SSH_AUTH_SOCK
    ///
    /// The vault must be unlocked: the daemon reads the staged identity from the
    /// keychain and never prompts. It refuses with "vault is locked; run bogita
    /// unlock" while the vault is locked, and with a message naming the socket
    /// while another daemon already answers on it.
    Start {
        /// Serve on this path instead of the default socket
        #[arg(long)]
        socket: Option<String>,
        #[arg(long, hide = true)]
        internal: bool,
    },
    /// Stop the agent daemon and remove its socket
    ///
    /// Exits 1 with "agent not running" when nothing answers on the socket.
    Stop,
    /// Show the agent status line
    ///
    /// One line: the socket path, "running", the number of served keys, and
    /// "locked" or "unlocked"; or the socket path and "not running". Exits 0 in
    /// both cases: this is a report command.
    Status,
    /// List the keys the agent serves
    ///
    /// One line per key: its SHA256 fingerprint and its comment. Exits 1 with
    /// "agent not running" when nothing answers on the socket.
    Keys,
}

/// Commands that run against an initialised app instead of the TUI.
///
/// The binary builds these from [`Commands`] in its dispatcher, so every value
/// reaching a handler is known to be valid — no defensive guard is needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    EntryLs {
        vault: Option<String>,
    },
    EntryGet {
        name: String,
        field: Option<String>,
        vault: Option<String>,
    },
    EntrySearch {
        query: String,
        vault: Option<String>,
    },
    EntryCp {
        name: String,
        field: String,
        vault: Option<String>,
    },
    EntryKeygen {
        comment: Option<String>,
        name: Option<String>,
        vault: Option<String>,
    },
    EntryRotate {
        name: String,
        comment: Option<String>,
        vault: Option<String>,
    },
    EntryPubkey {
        name: String,
        copy: bool,
        vault: Option<String>,
    },
    EntryServe {
        name: String,
        on: bool,
        vault: Option<String>,
    },
    VaultList,
    VaultDefault {
        name: String,
    },
    VaultRm {
        name: String,
    },
}

// region:    --- Tests
#[cfg(test)]
#[path = "../test/args.rs"]
mod args_test;
// endregion: --- Tests
