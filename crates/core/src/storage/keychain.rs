use std::sync::Mutex;

use crate::error::{CryptoError, Result};
use crate::ports::KeychainStore;

/// Service name of the OS keychain entry: `bogita` in release builds,
/// `bogita-dev` in debug builds — the compile-time external-state gate.
pub fn keychain_service() -> &'static str {
    #[cfg(debug_assertions)]
    let service = "bogita-dev";
    #[cfg(not(debug_assertions))]
    let service = "bogita";
    service
}

/// Account name of the OS keychain entry.
pub const KEYCHAIN_USER: &str = "age-identity";

/// Keychain adapter over the `keyring` crate.
///
/// The adapter owns no service/user knowledge: the [`keyring::Entry`] is built
/// by the composition root — production via [`platform_adapter`], tests via
/// [`KeychainAdapter::from_entry`] over a keyring-core mock credential — so the
/// environment is the only thing that differs between the two.
pub struct KeychainAdapter {
    entry: Mutex<Option<keyring::Entry>>,
}

impl KeychainAdapter {
    /// Build the adapter over an already-resolved keychain entry.
    pub fn from_entry(entry: keyring::Entry) -> Self {
        Self {
            entry: Mutex::new(Some(entry)),
        }
    }

    fn entry(&self) -> Result<std::sync::MutexGuard<'_, Option<keyring::Entry>>> {
        Ok(self.entry.lock().expect("KeychainAdapter mutex poisoned"))
    }
}

/// Build the production adapter over the platform credential store.
///
/// The single production construction seam: it resolves the platform keychain
/// entry once, so every caller (app init, ssh helpers) shares one construction
/// path and one set of service/user literals.
pub fn platform_adapter() -> Result<KeychainAdapter> {
    let entry = keyring::Entry::new(keychain_service(), KEYCHAIN_USER)
        .map_err(|e| CryptoError::KeychainError(e.to_string()))?;
    Ok(KeychainAdapter::from_entry(entry))
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

// region:    --- Tests
#[cfg(test)]
#[path = "../../test/storage/keychain.rs"]
mod keychain_test;
// endregion: --- Tests
