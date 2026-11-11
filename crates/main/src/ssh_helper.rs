use bogita_cli::handlers::ssh_sign::{self, HelperAction, HelperError};
use bogita_core::app::{DefaultApp, InitResult};
use bogita_core::session::Session;
use bogita_core::storage::keychain::platform_adapter;

/// Answer one `gpg.ssh.program` invocation.
///
/// The identity gate runs first, before the database is opened: a locked vault
/// refuses every invocation with the contract message and exit code 1.
pub(crate) async fn run_ssh_sign_helper(args: Vec<String>) {
    let session = keychain_session();
    let (identity, action) = match ssh_sign::prepare(&args, &session) {
        Ok(prepared) => prepared,
        Err(error) => exit_helper(&error),
    };

    match action {
        HelperAction::Forward(forwarded) => match ssh_sign::forward(&forwarded) {
            Ok(code) => std::process::exit(code),
            Err(error) => exit_helper(&error),
        },
        HelperAction::Sign(request) => {
            let app = ready_app().await;
            if let Err(error) = ssh_sign::handle_sign(&identity, &request, app.registry).await {
                exit_helper(&error);
            }
        }
    }
}

/// Print the public key line of the only agent-servable key.
pub(crate) async fn run_ssh_default_key() {
    let session = keychain_session();
    let identity = match ssh_sign::gate(&session) {
        Ok(identity) => identity,
        Err(error) => exit_helper(&error),
    };

    let app = ready_app().await;
    match ssh_sign::handle_default_key(&identity, app.registry).await {
        Ok(line) => println!("{line}"),
        Err(error) => exit_helper(&error),
    }
}

/// Session over the real keychain, built through the composition seam.
///
/// The helpers never prompt: an unusable platform keychain is reported as the
/// locked-vault contract message with exit code 1.
fn keychain_session() -> Session<bogita_core::storage::keychain::KeychainAdapter> {
    match platform_adapter() {
        Ok(adapter) => Session::new(adapter),
        Err(_) => exit_helper(&HelperError::Locked),
    }
}

/// Open the vault after the gate succeeded. Never prompts: an unexpected init
/// result is reported as a locked vault.
async fn ready_app() -> DefaultApp {
    match DefaultApp::init().await {
        InitResult::Ready(app) => app,
        _ => exit_helper(&HelperError::Locked),
    }
}

/// Report a helper failure on stderr and stop with exit code 1.
fn exit_helper(error: &HelperError) -> ! {
    eprintln!("{error}");
    std::process::exit(1);
}
