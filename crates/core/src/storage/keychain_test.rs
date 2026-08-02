use crate::ports::KeychainStore;
use crate::storage::keychain::KeychainAdapter;
use serial_test::serial;

/// Initialize the keyring mock store for testing.
/// This avoids touching the system keychain.
fn init_mock_store() {
    keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
}

#[test]
#[serial]
fn store_get_delete_roundtrip() {
    init_mock_store();
    let kc = KeychainAdapter::new();
    kc.delete_identity().unwrap();
    kc.store_identity("test-secret-key").unwrap();
    let retrieved = kc.get_identity().unwrap();
    assert_eq!(retrieved, Some("test-secret-key".to_string()));
    kc.delete_identity().unwrap();
    let after_delete = kc.get_identity().unwrap();
    assert_eq!(after_delete, None);
}

#[test]
#[serial]
fn get_returns_none_when_not_stored() {
    init_mock_store();
    let kc = KeychainAdapter::new();
    kc.delete_identity().unwrap();
    let result = kc.get_identity().unwrap();
    assert_eq!(result, None);
}

#[test]
#[serial]
fn delete_when_not_stored_is_ok() {
    init_mock_store();
    let kc = KeychainAdapter::new();
    kc.delete_identity().unwrap();
    kc.delete_identity().unwrap();
}

#[test]
#[serial]
fn store_overwrites_existing() {
    init_mock_store();
    let kc = KeychainAdapter::new();
    kc.delete_identity().unwrap();
    kc.store_identity("first").unwrap();
    kc.store_identity("second").unwrap();
    let retrieved = kc.get_identity().unwrap();
    assert_eq!(retrieved, Some("second".to_string()));
    kc.delete_identity().unwrap();
}
