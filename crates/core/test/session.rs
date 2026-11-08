type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::domain::AgeIdentity;
use crate::session::Session;
use crate::test_support::MockKeychain;

#[test]
fn store_identity_stores_in_keychain() -> Result<()> {
    let kc = MockKeychain::new();
    let session = Session::new(kc);
    let identity = AgeIdentity::generate();
    session.store_identity(&identity)?;
    assert!(session.get_identity()?.is_some());
    Ok(())
}

#[test]
fn lock_removes_identity_from_keychain() -> Result<()> {
    let kc = MockKeychain::new();
    let session = Session::new(kc);
    let identity = AgeIdentity::generate();
    session.store_identity(&identity)?;
    assert!(session.get_identity()?.is_some());
    session.lock()?;
    assert!(session.get_identity()?.is_none());
    Ok(())
}

#[test]
fn get_identity_returns_none_when_locked() -> Result<()> {
    let kc = MockKeychain::new();
    let session = Session::new(kc);
    assert!(session.get_identity()?.is_none());
    Ok(())
}

#[test]
fn get_identity_returns_some_when_unlocked() -> Result<()> {
    let kc = MockKeychain::new();
    let session = Session::new(kc);
    let identity = AgeIdentity::generate();
    session.store_identity(&identity)?;
    let retrieved = session.get_identity()?;
    assert!(retrieved.is_some());
    assert_eq!(
        retrieved.ok_or("should be some")?.to_recipient(),
        identity.to_recipient()
    );
    Ok(())
}

#[test]
fn lock_when_already_locked_is_ok() -> Result<()> {
    let kc = MockKeychain::new();
    let session = Session::new(kc);
    session.lock()?;
    Ok(())
}
