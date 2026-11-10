use crate::exit_with_error;
use bogita_cli::handlers::agent::{
    handle_keys, handle_start, handle_status, handle_stop, socket_path,
};
use bogita_core::agent::{serve_with_signals, KeyRing, LockMonitor, SshAgentError};
use bogita_core::app::{DefaultApp, InitResult};
use std::sync::{Arc, Mutex};

/// Start the agent daemon and print the socket the client must export.
///
/// The vault must be unlocked: the daemon reads the staged identity from the
/// keychain, so a vault without a staged identity refuses the start instead of
/// prompting.
pub(crate) async fn run_agent_start(socket: Option<String>) {
    match DefaultApp::init().await {
        InitResult::Ready(_) => match handle_start(socket) {
            Ok(path) => println!("SSH_AUTH_SOCK={}", path.display()),
            Err(SshAgentError::AlreadyServing(path)) => {
                eprintln!("agent already running on {path}");
                std::process::exit(1);
            }
            Err(e) => exit_with_error(&e.to_string()),
        },
        InitResult::Locked(_) => refuse_agent_start("vault is locked; run bogita unlock"),
        InitResult::NeedsPassphrase(_) => {
            refuse_agent_start("vault is not initialized; run bogita unlock to set up")
        }
        InitResult::Failed { message } => exit_with_error(&message),
    }
}

/// Serve the agent in this process — the hidden `--internal` child.
pub(crate) async fn run_agent_daemon(socket: Option<String>) {
    let app = match DefaultApp::init().await {
        InitResult::Ready(app) => app,
        InitResult::Locked(_) => refuse_agent_start("vault is locked; run bogita unlock"),
        InitResult::NeedsPassphrase(_) => {
            refuse_agent_start("vault is not initialized; run bogita unlock to set up")
        }
        InitResult::Failed { message } => exit_with_error(&message),
    };

    let entries = app
        .all_entries()
        .await
        .unwrap_or_else(|e| exit_with_error(&e.to_string()));
    let mut ring = KeyRing::new();
    ring.load_entries(entries);
    let ring = Arc::new(Mutex::new(ring));

    // The lock watcher is the lock-state source: it polls the staged keychain item
    // and reloads or evicts the ring. It stops when the daemon exits.
    let monitor = LockMonitor::new(
        app.session,
        app.lock_timeout,
        Arc::clone(&ring),
        app.registry,
    );
    let watching = tokio::spawn(monitor.run());

    let served = serve_with_signals(ring, socket_path(socket)).await;
    watching.abort();
    if let Err(e) = served {
        exit_with_error(&e.to_string());
    }
}

/// Report a start attempt with no staged identity.
fn refuse_agent_start(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}

/// Stop the agent daemon.
pub(crate) fn run_agent_stop() {
    match handle_stop(None) {
        Ok(()) => println!("agent stopped"),
        Err(SshAgentError::NotRunning(_)) => {
            eprintln!("agent not running");
            std::process::exit(1);
        }
        Err(e) => exit_with_error(&e.to_string()),
    }
}

/// Print the agent status line.
pub(crate) fn run_agent_status() {
    match handle_status(None) {
        Ok(line) => println!("{line}"),
        Err(e) => exit_with_error(&e.to_string()),
    }
}

/// Print one line per served key.
pub(crate) fn run_agent_keys() {
    match handle_keys(None) {
        Ok(lines) => {
            for line in lines {
                println!("{line}");
            }
        }
        Err(SshAgentError::NotRunning(_)) => {
            eprintln!("agent not running");
            std::process::exit(1);
        }
        Err(e) => exit_with_error(&e.to_string()),
    }
}
