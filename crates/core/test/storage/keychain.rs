use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use secrecy::ExposeSecret;

use crate::domain::AgeIdentity;
use crate::error::{CryptoError, Error};
use crate::ports::KeychainStore;
use crate::session::Session;
use crate::storage::keychain::KeychainAdapter;
use crate::test_support::MockKeychain;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

// ── fixtures ─────────────────────────────────────────────────────────────────

/// An in-memory keyring-core mock credential for the given service specifier.
///
/// The adapter is built from it through the SAME `from_entry` seam production
/// uses at the composition root — no platform store, no default-store seam and
/// no test-only adapter API.
fn mock_cred(service: &str) -> Arc<keyring_core::mock::Cred> {
    Arc::new(keyring_core::mock::Cred {
        specifiers: (service.to_string(), "age-identity".to_string()),
        inner: Mutex::new(RefCell::new(keyring_core::mock::CredData::default())),
    })
}

/// The real adapter over an injected mock credential.
fn adapter_with(cred: &Arc<keyring_core::mock::Cred>) -> KeychainAdapter {
    KeychainAdapter::from_entry(keyring::Entry {
        inner: keyring_core::Entry::new_with_credential(cred.clone()),
    })
}

// ── adapter: its own logic, exercised headlessly ──────────────────────────────

#[test]
fn test_adapter_store_get_delete_roundtrip() -> Result<()> {
    // -- Setup & Fixtures
    let cred = mock_cred("bogita-test");
    let kc = adapter_with(&cred);

    // -- Exec
    kc.store_identity("test-secret-key")?;
    let retrieved = kc.get_identity()?;
    kc.delete_identity()?;
    let after_delete = kc.get_identity()?;

    // -- Check
    assert_eq!(retrieved, Some("test-secret-key".to_string()));
    assert_eq!(after_delete, None);
    assert_eq!(cred.specifiers.0, "bogita-test");
    assert_eq!(cred.specifiers.1, "age-identity");
    Ok(())
}

#[test]
fn test_adapter_get_returns_none_when_not_stored() -> Result<()> {
    // -- Setup & Fixtures
    let cred = mock_cred("bogita-test");
    let kc = adapter_with(&cred);

    // -- Exec & Check
    assert_eq!(kc.get_identity()?, None);
    Ok(())
}

#[test]
fn test_adapter_delete_when_not_stored_is_ok() -> Result<()> {
    // -- Setup & Fixtures
    let cred = mock_cred("bogita-test");
    let kc = adapter_with(&cred);

    // -- Exec & Check
    assert!(kc.delete_identity().is_ok());
    Ok(())
}

#[test]
fn test_adapter_store_overwrites_last_wins() -> Result<()> {
    // -- Setup & Fixtures
    let cred = mock_cred("bogita-test");
    let kc = adapter_with(&cred);

    // -- Exec
    kc.store_identity("first")?;
    kc.store_identity("second")?;

    // -- Check
    assert_eq!(kc.get_identity()?, Some("second".to_string()));
    Ok(())
}

#[test]
fn test_adapter_error_is_wrapped_as_keychain_error() -> Result<()> {
    // -- Setup & Fixtures
    let cred = mock_cred("bogita-test");
    let kc = adapter_with(&cred);
    cred.set_error(keyring_core::Error::Invalid(
        "service".to_string(),
        "mock failure".to_string(),
    ));

    // -- Exec
    let err = match kc.store_identity("nope") {
        Ok(()) => return Err("the injected mock error must surface".into()),
        Err(e) => e,
    };

    // -- Check
    assert!(matches!(err, Error::Crypto(CryptoError::KeychainError(_))));
    Ok(())
}

// ── port consumers: Session over the in-memory port mock ──────────────────────

#[test]
fn test_session_store_and_get_identity_roundtrip() -> Result<()> {
    // -- Setup & Fixtures
    let session = Session::new(MockKeychain::new());
    let identity = AgeIdentity::generate();

    // -- Exec
    session.store_identity(&identity)?;
    let stored = session.get_identity()?;

    // -- Check
    let stored = stored.ok_or("identity should be stored")?;
    assert_eq!(
        stored.to_secret_string().expose_secret(),
        identity.to_secret_string().expose_secret()
    );
    Ok(())
}

#[test]
fn test_session_get_identity_returns_none_when_not_stored() -> Result<()> {
    // -- Setup & Fixtures
    let session = Session::new(MockKeychain::new());

    // -- Exec & Check
    assert!(session.get_identity()?.is_none());
    Ok(())
}

#[test]
fn test_session_lock_when_not_stored_is_ok() -> Result<()> {
    // -- Setup & Fixtures
    let session = Session::new(MockKeychain::new());

    // -- Exec & Check
    session.lock()?;
    Ok(())
}

#[test]
fn test_session_lock_removes_stored_identity() -> Result<()> {
    // -- Setup & Fixtures
    let session = Session::new(MockKeychain::new());
    session.store_identity(&AgeIdentity::generate())?;

    // -- Exec
    session.lock()?;

    // -- Check
    assert!(session.get_identity()?.is_none());
    Ok(())
}

// ── static guards: no platform seam in the test tree ─────────────────────────

fn core_crate_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_crates_root() -> std::path::PathBuf {
    let root = core_crate_root();
    match root.parent().and_then(|p| p.parent()) {
        Some(workspace) => workspace.join("crates"),
        None => root,
    }
}

fn rust_files_under(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    files
}

/// Source files under `root` whose text contains `needle`.
fn find_violations(root: &std::path::Path, needle: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for file in rust_files_under(root) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        if text.contains(needle) {
            hits.push(file.display().to_string());
        }
    }
    hits
}

#[test]
fn test_static_test_tree_has_no_default_store_seam() {
    // The needles are built with `concat!` so this source cannot match itself.
    let hits = find_violations(
        &core_crate_root().join("test"),
        concat!("set_default", "_store"),
    );
    assert!(
        hits.is_empty(),
        "the test tree must never install a keyring default store: {hits:?}"
    );
}

#[test]
fn test_static_test_tree_has_no_platform_entry_resolution() {
    // The needles are built with `concat!` so this source cannot match itself.
    let hits = find_violations(&core_crate_root().join("test"), concat!("Entry::new", "("));
    assert!(
        hits.is_empty(),
        "tests must inject a credential instead of resolving a platform entry: {hits:?}"
    );
}

#[test]
fn test_static_adapter_has_no_other_constructor() {
    // The needles are built with `concat!` so this source cannot match itself.
    let hits = find_violations(
        &workspace_crates_root(),
        concat!("KeychainAdapter", "::", "new"),
    );
    assert!(
        hits.is_empty(),
        "KeychainAdapter is built by from_entry/platform_adapter only: {hits:?}"
    );
}
