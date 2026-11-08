use crate::ports::KeychainStore;
use crate::storage::keychain::KeychainAdapter;
use serial_test::serial;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

/// Initialize the keyring-core mock store for testing.
/// This avoids touching the system keychain.
fn init_mock_store() -> Result<()> {
    // The v1 layer installs the platform store as the keyring-core default on
    // first use, so trigger that one-time init first and then override it with
    // the in-memory mock.
    let _ = keyring::Entry::store_status();
    keyring_core::set_default_store(keyring_core::mock::Store::new()?);
    Ok(())
}

#[test]
#[serial]
fn store_get_delete_roundtrip() -> Result<()> {
    // -- Setup & Fixtures
    init_mock_store()?;
    let kc = KeychainAdapter::new();
    kc.delete_identity()?;
    kc.store_identity("test-secret-key")?;

    // -- Exec
    let retrieved = kc.get_identity()?;
    kc.delete_identity()?;
    let after_delete = kc.get_identity()?;

    // -- Check
    assert_eq!(retrieved, Some("test-secret-key".to_string()));
    assert_eq!(after_delete, None);
    Ok(())
}

#[test]
#[serial]
fn get_returns_none_when_not_stored() -> Result<()> {
    // -- Setup & Fixtures
    init_mock_store()?;
    let kc = KeychainAdapter::new();
    kc.delete_identity()?;

    // -- Exec
    let result = kc.get_identity()?;

    // -- Check
    assert_eq!(result, None);
    Ok(())
}

#[test]
#[serial]
fn delete_when_not_stored_is_ok() -> Result<()> {
    // -- Setup & Fixtures
    init_mock_store()?;
    let kc = KeychainAdapter::new();
    kc.delete_identity()?;

    // -- Exec
    let result = kc.delete_identity();

    // -- Check
    assert!(result.is_ok());
    Ok(())
}

#[test]
#[serial]
fn store_overwrites_existing() -> Result<()> {
    // -- Setup & Fixtures
    init_mock_store()?;
    let kc = KeychainAdapter::new();
    kc.delete_identity()?;
    kc.store_identity("first")?;
    kc.store_identity("second")?;

    // -- Exec
    let retrieved = kc.get_identity()?;

    // -- Check
    assert_eq!(retrieved, Some("second".to_string()));
    kc.delete_identity()?;
    Ok(())
}
