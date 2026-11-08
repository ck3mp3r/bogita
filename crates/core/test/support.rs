//! Shared test helpers for bogita-core.
//!
//! Provides [`MockKeychain`], an in-memory implementation of [`KeychainStore`]
//! that avoids touching the OS keychain. All test code should use this instead
//! of the `keyring-core` mock store.
//!
//! Also provides fixture builders for domain values and [`seed_vault`], a
//! vault-row insert so tests can satisfy foreign-key constraints without
//! building a full `Vault` with recipients.

use crate::domain::{
    AgeIdentity, AgeRecipient, Entry, EntryType, Field, FieldType, FieldValue, Vault,
};
use crate::ports::KeychainStore;
use std::sync::Mutex;
use uuid::Uuid;

#[cfg(feature = "integration")]
use crate::error::Result;
#[cfg(feature = "integration")]
use crate::ports::VaultStore;

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

/// Build a non-default vault with no recipients used by fixtures.
pub fn test_vault(name: &str) -> Vault {
    test_vault_with(Uuid::new_v4(), name, false, vec![])
}

/// Build a vault with explicit id, default flag, and recipients.
pub fn test_vault_with(
    id: Uuid,
    name: &str,
    is_default: bool,
    recipients: Vec<AgeRecipient>,
) -> Vault {
    Vault {
        id,
        name: name.to_string(),
        is_default,
        created_at: chrono::Utc::now().timestamp(),
        sync_target: None,
        recipients,
        lock_timeout: None,
        auto_sync: false,
    }
}

/// Build a vault bound to one fresh recipient with a 300 s lock timeout.
pub fn test_vault_locked(name: &str, is_default: bool) -> Vault {
    // Divergence from test_vault_with kept on purpose: fresh recipient plus lock_timeout 300 s.
    let identity = AgeIdentity::generate();
    let mut vault = test_vault_with(
        Uuid::new_v4(),
        name,
        is_default,
        vec![identity.to_recipient()],
    );
    vault.lock_timeout = Some(300);
    vault
}

/// Build a field for an entry fixture.
pub fn test_field(
    key: &str,
    value: FieldValue,
    field_type: FieldType,
    encrypted: bool,
    idx: i32,
) -> Field {
    Field {
        id: Uuid::new_v4(),
        key: key.to_string(),
        value,
        field_type,
        encrypted,
        idx,
    }
}

/// Build a token entry with the given fields.
pub fn test_entry(vault_id: Uuid, name: &str, fields: Vec<Field>) -> Entry {
    Entry {
        id: Uuid::new_v4(),
        vault_id,
        name: name.to_string(),
        entry_type: EntryType::Token,
        created_at: chrono::Utc::now().timestamp(),
        modified_at: chrono::Utc::now().timestamp(),
        fields,
    }
}

/// Build a token entry with a username and a hidden password.
pub fn test_password_entry(vault_id: Uuid, name: &str) -> Entry {
    test_entry(
        vault_id,
        name,
        vec![
            test_field(
                "username",
                FieldValue::Text("alice".to_string()),
                FieldType::Username,
                false,
                0,
            ),
            test_field(
                "password",
                FieldValue::Hidden("s3cr3t".to_string()),
                FieldType::Token,
                true,
                1,
            ),
        ],
    )
}

/// Build an OTP entry with a TOTP secret field.
pub fn test_otp_entry(vault_id: Uuid, name: &str) -> Entry {
    Entry {
        entry_type: EntryType::Otp,
        ..test_entry(
            vault_id,
            name,
            vec![test_field(
                "secret",
                FieldValue::Hidden("JBSWY3DPEHPK3PXP".to_string()),
                FieldType::TotpSecret,
                true,
                0,
            )],
        )
    }
}

/// Insert a minimal vault row so FK constraints pass in tests.
///
/// Goes through the public `save_vault` API; the vault id doubles as the name
/// so the row stays unique without a caller-supplied name.
#[cfg(feature = "integration")]
pub async fn seed_vault(storage: &impl VaultStore, vault_id: Uuid) -> Result<()> {
    let vault = test_vault_with(vault_id, &vault_id.to_string(), false, vec![]);
    storage.save_vault(&vault).await
}
