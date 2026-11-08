use crate::ports::KeychainStore;
use crate::storage::keychain::KeychainAdapter;
use serial_test::serial;

/// Initialize the keyring-core mock store for testing.
/// This avoids touching the system keychain.
fn init_mock_store() {
    // The v1 layer installs the platform store as the keyring-core default on
    // first use, so trigger that one-time init first and then override it with
    // the in-memory mock.
    let _ = keyring::Entry::store_status();
    keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
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
