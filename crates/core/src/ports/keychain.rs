use crate::error::Result;

/// Port trait for storing and retrieving the age secret key from the OS keychain.
///
/// The keychain stores the raw age secret key string (AGE-SECRET-KEY-1...) so
/// that CLI commands and the TUI can retrieve it without prompting for the
/// passphrase on every invocation.
///
/// - Lock = remove from keychain (delete_identity)
/// - Unlock = store in keychain (store_identity)
/// - Is locked = keychain has no entry (get_identity returns None)
pub trait KeychainStore {
    /// Store the age secret key string in the keychain. Overwrites any existing entry.
    fn store_identity(&self, secret_key: &str) -> Result<()>;

    /// Retrieve the age secret key string from the keychain.
    /// Returns `None` if no entry exists (vault is locked).
    fn get_identity(&self) -> Result<Option<String>>;

    /// Remove the identity from the keychain (lock).
    /// Idempotent — deleting when already deleted is OK.
    fn delete_identity(&self) -> Result<()>;
}
