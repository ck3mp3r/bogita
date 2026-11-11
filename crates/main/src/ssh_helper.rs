use bogita_cli::handlers::ssh_relay::{self, DaemonRelay};
use bogita_cli::handlers::ssh_sign::{self, HelperAction, HelperError};
use bogita_core::agent::client;
use bogita_core::agent::socket::default_socket_path;

/// Answer one `gpg.ssh.program` invocation.
///
/// The lock gate runs first, before any file work: the daemon owns the lock, so a
/// missing or locked daemon refuses every invocation with the contract message
/// and exit code 1.
pub(crate) async fn run_ssh_sign_helper(args: Vec<String>) {
    let action = match ssh_sign::prepare(daemon_status().as_deref(), &args) {
        Ok(action) => action,
        Err(error) => exit_helper(&error),
    };

    match action {
        HelperAction::Forward(forwarded) => match ssh_sign::forward(&forwarded) {
            Ok(code) => std::process::exit(code),
            Err(error) => exit_helper(&error),
        },
        // The daemon signs from its ring: this process reads the requested public
        // key, matches it against the advertised identities, and writes the armor.
        HelperAction::Sign(request) => {
            let mut relay = DaemonRelay::new(default_socket_path());
            match ssh_relay::sign(&mut relay, &request) {
                Ok(()) => {}
                Err(error) => exit_helper(&error),
            }
        }
    }
}

/// Print the public key line of the only key the daemon serves.
pub(crate) async fn run_ssh_default_key() {
    match ssh_sign::gate(daemon_status().as_deref()) {
        Ok(()) => {
            let mut relay = DaemonRelay::new(default_socket_path());
            match ssh_relay::default_key(&mut relay) {
                Ok(line) => println!("{line}"),
                Err(error) => exit_helper(&error),
            }
        }
        Err(error) => exit_helper(&error),
    }
}

/// The daemon's status payload, or `None` when nothing answers.
///
/// An unreadable status counts as no daemon: the helper fails secure.
fn daemon_status() -> Option<String> {
    client::status(&default_socket_path()).ok().flatten()
}

/// Report a helper failure on stderr and stop with exit code 1.
fn exit_helper(error: &HelperError) -> ! {
    eprintln!("{error}");
    std::process::exit(1);
}
