use std::sync::Mutex;

use crate::error::{CryptoError, Result};
use crate::ports::KeychainStore;

/// Keychain adapter using the `keyring` crate.
/// Service name is "bogita" (or "bogita-dev" in debug builds).
pub struct KeychainAdapter {
    service: String,
    entry: Mutex<Option<keyring::Entry>>,
}

impl KeychainAdapter {
    pub fn new() -> Self {
        #[cfg(debug_assertions)]
        let service = "bogita-dev";
        #[cfg(not(debug_assertions))]
        let service = "bogita";
        Self {
            service: service.to_string(),
            entry: Mutex::new(None),
        }
    }

    fn entry(&self) -> Result<std::sync::MutexGuard<'_, Option<keyring::Entry>>> {
        let mut guard = self.entry.lock().expect("KeychainAdapter mutex poisoned");
        if guard.is_none() {
            let e = keyring::Entry::new(&self.service, "age-identity")
                .map_err(|e| CryptoError::KeychainError(e.to_string()))?;
            *guard = Some(e);
        }
        Ok(guard)
    }
}

impl Default for KeychainAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl KeychainStore for KeychainAdapter {
    fn store_identity(&self, secret_key: &str) -> Result<()> {
        let guard = self.entry()?;
        let entry = guard.as_ref().ok_or_else(|| {
            CryptoError::KeychainError("keychain entry not initialized".to_string())
        })?;
        entry
            .set_password(secret_key)
            .map_err(|e| CryptoError::KeychainError(e.to_string()))?;
        Ok(())
    }

    fn get_identity(&self) -> Result<Option<String>> {
        let guard = self.entry()?;
        let entry = guard.as_ref().ok_or_else(|| {
            CryptoError::KeychainError("keychain entry not initialized".to_string())
        })?;
        match entry.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CryptoError::KeychainError(e.to_string()).into()),
        }
    }

    fn delete_identity(&self) -> Result<()> {
        let guard = self.entry()?;
        let entry = guard.as_ref().ok_or_else(|| {
            CryptoError::KeychainError("keychain entry not initialized".to_string())
        })?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CryptoError::KeychainError(e.to_string()).into()),
        }
    }
}
