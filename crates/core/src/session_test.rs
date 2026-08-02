use crate::domain::AgeIdentity;
use crate::session::Session;
use crate::test_helpers::MockKeychain;

#[test]
fn store_identity_stores_in_keychain() {
    let kc = MockKeychain::new();
    let session = Session::new(kc);
    let identity = AgeIdentity::generate();
    session.store_identity(&identity).unwrap();
    assert!(!session.is_locked().unwrap());
}

#[test]
fn lock_removes_identity_from_keychain() {
    let kc = MockKeychain::new();
    let session = Session::new(kc);
    let identity = AgeIdentity::generate();
    session.store_identity(&identity).unwrap();
    assert!(!session.is_locked().unwrap());
    session.lock().unwrap();
    assert!(session.is_locked().unwrap());
}

#[test]
fn get_identity_returns_none_when_locked() {
    let kc = MockKeychain::new();
    let session = Session::new(kc);
    assert!(session.is_locked().unwrap());
    assert!(session.get_identity().unwrap().is_none());
}

#[test]
fn get_identity_returns_some_when_unlocked() {
    let kc = MockKeychain::new();
    let session = Session::new(kc);
    let identity = AgeIdentity::generate();
    session.store_identity(&identity).unwrap();
    let retrieved = session.get_identity().unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().to_recipient(), identity.to_recipient());
}

#[test]
fn lock_when_already_locked_is_ok() {
    let kc = MockKeychain::new();
    let session = Session::new(kc);
    session.lock().unwrap();
}
