use crate::domain::AgeIdentity;
use crate::error::{CryptoError, Result};
use crate::ports::KeychainStore;
use secrecy::ExposeSecret;
use std::str::FromStr;

pub struct Session<K: KeychainStore> {
    keychain: K,
}

impl<K: KeychainStore> Session<K> {
    pub fn new(keychain: K) -> Self {
        Self { keychain }
    }

    pub fn lock(&self) -> Result<()> {
        self.keychain.delete_identity()
    }

    pub fn is_locked(&self) -> Result<bool> {
        Ok(self.keychain.get_identity()?.is_none())
    }

    pub fn store_identity(&self, identity: &AgeIdentity) -> Result<()> {
        let secret = identity.to_secret_string();
        self.keychain.store_identity(secret.expose_secret())
    }

    pub fn get_identity(&self) -> Result<Option<AgeIdentity>> {
        match self.keychain.get_identity()? {
            None => Ok(None),
            Some(secret_str) => {
                let identity =
                    AgeIdentity::from_str(&secret_str).map_err(CryptoError::InvalidIdentity)?;
                Ok(Some(identity))
            }
        }
    }
}

#[cfg(test)]
#[path = "session_test.rs"]
mod session_test;
