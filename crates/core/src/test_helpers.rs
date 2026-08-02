//! Shared test helpers for bogita-core.
//!
//! Provides [`MockKeychain`], an in-memory implementation of [`KeychainStore`]
//! that avoids touching the OS keychain. All test code should use this instead
//! of `keyring::mock`.

use crate::ports::KeychainStore;
use std::sync::Mutex;

/// In-memory keychain for testing. Never touches the OS keychain.
pub struct MockKeychain {
    inner: Mutex<Option<String>>,
}

impl MockKeychain {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

impl Default for MockKeychain {
    fn default() -> Self {
        Self::new()
    }
}

impl KeychainStore for MockKeychain {
    fn store_identity(&self, secret_key: &str) -> crate::error::Result<()> {
        *self.inner.lock().expect("mock keychain mutex poisoned") = Some(secret_key.to_string());
        Ok(())
    }

    fn get_identity(&self) -> crate::error::Result<Option<String>> {
        Ok(self
            .inner
            .lock()
            .expect("mock keychain mutex poisoned")
            .clone())
    }

    fn delete_identity(&self) -> crate::error::Result<()> {
        *self.inner.lock().expect("mock keychain mutex poisoned") = None;
        Ok(())
    }
}
