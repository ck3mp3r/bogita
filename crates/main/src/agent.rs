use crate::exit_with_error;
use bogita_cli::handlers::agent::{
    handle_keys, handle_start, handle_status, handle_stop, socket_path,
};
use bogita_core::agent::unlock::IdleWatch;
use bogita_core::agent::{serve_with_signals, KeyRing, SshAgentError, VaultLock};
use bogita_core::app::{App, DefaultApp, InitResult};
use std::sync::{Arc, Mutex};

/// Start the agent daemon and print the socket the client must export.
///
/// The daemon boots locked and waits, so an initialised vault is enough to
/// start it; an uninitialised one has nothing to serve.
pub(crate) async fn run_agent_start(socket: Option<String>) {
    match DefaultApp::init().await {
        InitResult::Locked(_) => match handle_start(socket) {
            Ok(path) => println!("SSH_AUTH_SOCK={}", path.display()),
            Err(SshAgentError::AlreadyServing(path)) => {
                eprintln!("agent already running on {path}");
                std::process::exit(1);
            }
            Err(e) => exit_with_error(&e.to_string()),
        },
        InitResult::NeedsPassphrase(_) => {
            refuse_agent_start("vault is not initialized; run bogita unlock to set up")
        }
        InitResult::Failed { message } => exit_with_error(&message),
    }
}

/// Serve the agent in this process — the hidden `--internal` child.
///
/// The daemon boots LOCKED, whatever the vault state on disk: it holds no
/// identity and serves no key until an unlock request arrives on the control
/// channel. The unlock derives the identity from the encrypted file, so the
/// daemon never reads a credential store.
pub(crate) async fn run_agent_daemon(socket: Option<String>) {
    let ring = Arc::new(Mutex::new(KeyRing::new()));
    let (service, lock_timeout) = match DefaultApp::init().await {
        InitResult::Locked(parts) => {
            let App {
                config,
                identity_store,
                registry,
                lock_timeout,
                ..
            } = App::pending(parts);
            let service = VaultLock::new(
                config.effective_identity_path(),
                identity_store,
                registry,
                Arc::clone(&ring),
            );
            (service, lock_timeout)
        }
        InitResult::NeedsPassphrase(_) => {
            refuse_agent_start("vault is not initialized; run bogita unlock to set up")
        }
        InitResult::Failed { message } => exit_with_error(&message),
    };

    // The idle watch is the daemon's own inactivity cap over the lock it serves.
    // Idle eviction is the service's lock: it drops the identity with the ring, so
    // an idle daemon holds no key material at all.
    let service = Arc::new(service);
    let idle = IdleWatch::new(lock_timeout, Arc::clone(&ring), Arc::clone(&service));
    let watching = tokio::spawn(idle.run());

    let served = serve_with_signals(ring, socket_path(socket), service).await;
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
