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
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[cfg(feature = "integration")]
use crate::error::Result;
#[cfg(feature = "integration")]
use crate::ports::VaultStore;

/// In-memory keychain for testing. Never touches the OS keychain.
///
/// The handle is shared, so a test can keep one clone to flip the item and hand
/// another to the code under test.
#[derive(Clone, Default)]
pub struct MockKeychain {
    inner: Arc<Mutex<Option<String>>>,
}

impl MockKeychain {
    pub fn new() -> Self {
        Self::default()
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

/// Unencrypted RSA OpenSSH private key, generated once offline with
/// `ssh-keygen -t rsa -b 2048 -N '' -C 'rsa-fixture@example.com'`.
///
/// The daemon matrix does not serve RSA yet: this fixture pins "the PEM parses
/// as RSA and the agent refuses it". Never generate key material at test
/// runtime.
pub const RSA_OPENSSH_PEM: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAABFwAAAAdzc2gtcn\n\
NhAAAAAwEAAQAAAQEA8c22C5JDPFgVBKSs+oq4gAASpeodt8GaTeQ5JnWZcHcdyxPEszvq\n\
3iM75Xohyg/9T3t4cJY7mIzn/5U0USYhdWRTGSrXHSH0jR+8TfwW742eCGGCUjuMuzz8Fs\n\
G8EjTzxiz7YiuxgnHPCBSLOK8zei41sSC7evCqMfXFvFgWUl9ONu0PBt5roFE6Vv5D46TO\n\
+vKyLqzj4ZWBEiolEoqrp15Geaqdi/tWHuOOE3jKvxyiUnOhf7tJ9mUhVRGdzYV2aoCmnT\n\
BhNF/qda9SY7T0No0UYCFcN2+sYV9bD1Dak2e77QEhjiGbZlhcGAQtCFn1ze2oZuT1T4AD\n\
xOBy37z2CwAAA9AeHvCIHh7wiAAAAAdzc2gtcnNhAAABAQDxzbYLkkM8WBUEpKz6iriAAB\n\
Kl6h23wZpN5DkmdZlwdx3LE8SzO+reIzvleiHKD/1Pe3hwljuYjOf/lTRRJiF1ZFMZKtcd\n\
IfSNH7xN/BbvjZ4IYYJSO4y7PPwWwbwSNPPGLPtiK7GCcc8IFIs4rzN6LjWxILt68Kox9c\n\
W8WBZSX0427Q8G3mugUTpW/kPjpM768rIurOPhlYESKiUSiqunXkZ5qp2L+1Ye444TeMq/\n\
HKJSc6F/u0n2ZSFVEZ3NhXZqgKadMGE0X+p1r1JjtPQ2jRRgIVw3b6xhX1sPUNqTZ7vtAS\n\
GOIZtmWFwYBC0IWfXN7ahm5PVPgAPE4HLfvPYLAAAAAwEAAQAAAQA7BS+cpnBD3+oNbpaM\n\
NuTu1VW36qujSR29SmoKsaSot2JtyZ728raSzv3VNtjdhcaaKnSM4pacVzRfhY82CX0cNs\n\
czuumzwjB7a9YeINyGT0ZWHFrfcIqavzl24oXh2cvwwNUBMtIKmXXjroQG63jAp1vSpr8s\n\
NGcOd8Nyg/u60fABIaZVA/fEV0DC5+8OVx+UiCEYd0SKEwaV5f87+aDCtPoFP5LtjFgAsJ\n\
4kDpuJ/cjkcGOkA5tFpRTuyOEJXH0tXKjHr5ZhUlkyaFFFhjaHW+45EWp07MYkpK9x8gaR\n\
nTA3Nt0ZOMvVhZHE002ssi99rcgh04Iq1/Atuv3p/c8pAAAAgQD7DRBO9qt0Nps1HKE8KC\n\
OBYpo+fIiaguuaepfb5b7Y5zIjLQFiPWzE0yrXTuIobqsVKbw2z2e5jNHSvwvga/t7fYMU\n\
uqBZ5b/r9GazUOlYbbeuSu8weh3Ler6u+REMBA+Iv6XQWktciWJYDjPWkUcLS3rCECiPWF\n\
RiklzVajlfEQAAAIEA+/rPlx7cMqD3e/Y+BSej0V5Bnuip11iyPpvsQNWIQkEVcZdUNBwy\n\
fP3nwhL+pX7LwnrL65hQIPgq5FCpt/FCdx5EZoVxET2ZiooMuBJ9qR/wHf0Ejg/E6MbJ1U\n\
lRguquK/DwB0Tbe58W4+ERP+lOO1bororcKwNErQQaf0KssBMAAACBAPWpVidDEmG0lwLt\n\
J5BzrmHCbr8qYlFWF3HJExaLt4L1tD5R0QD85B4ICD4IMeFSon59UaBfcN5pqU0orDxW53\n\
Tk1t+432gEyEVTl/tLAcLA2+kxOjYZqQjcPb3vhNwBe/GVYxdzxdz5gh58FTjFE40Uxw++\n\
GPhJRPsK0ZeDfpEpAAAAF3JzYS1maXh0dXJlQGV4YW1wbGUuY29tAQID\n\
-----END OPENSSH PRIVATE KEY-----\n";

/// Unencrypted ECDSA P-256 OpenSSH private key, generated once offline with
/// `ssh-keygen -t ecdsa -b 256 -N '' -C 'ecdsa-fixture@example.com'`.
///
/// The agent serves Ed25519 and RSA only: this fixture pins "the PEM parses and
/// the support matrix refuses the algorithm". Never generate key material at
/// test runtime.
pub const ECDSA_OPENSSH_PEM: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAaAAAABNlY2RzYS\n\
1zaGEyLW5pc3RwMjU2AAAACG5pc3RwMjU2AAAAQQQs3oK6qBQDNiXjrU1HXt/gocLnqpg0\n\
O3vdQW1hXPZsGQd4n39GmxtkvsocBw9UKKEzaN4zCk8pAAvkF63l7hraAAAAuNwaKK3cGi\n\
itAAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBCzegrqoFAM2JeOt\n\
TUde3+ChwueqmDQ7e91BbWFc9mwZB3iff0abG2S+yhwHD1QooTNo3jMKTykAC+QXreXuGt\n\
oAAAAgHds7kNkx0nsEpL0CoZ3FYb9FVReVvCCClbqpKPfc0YYAAAAZZWNkc2EtZml4dHVy\n\
ZUBleGFtcGxlLmNvbQECAwQFBgc=\n\
-----END OPENSSH PRIVATE KEY-----\n";

/// An age-encrypted file blob shaped like the values that once reached
/// `SshPrivateKey` fields (live rows decoded to `age-encryption.org/v1`).
///
/// Age files are ciphertext, not OpenSSH keys: no library converts them into a
/// private key. This fixture pins "the doors refuse a value that is not an
/// OpenSSH PEM". It never decrypts.
pub const AGE_ENCRYPTED_FILE: &str = "-----BEGIN AGE ENCRYPTED FILE-----\n\
age-encryption.org/v1\n\
-> X25519 1sVJx0n4YQmT0bGZt3Vq7Yk2f9Ld5Rc8Wm6Np4Ks1Xy\n\
7QmT0bGZt3Vq7Yk2f9Ld5Rc8Wm6Np4Ks1Xy7QmT0bGZt3Vq7Yk2f9Ld5Rc8Wm6Np\n\
4Ks1Xy7QmT0bGZt3Vq7Yk2f9Ld5Rc8Wm6Np4Ks1Xy7QmT0bGZt3Vq7Yk2f9Ld5Rc8W\n\
m6Np4Ks1Xy7QmT0bGZt3Vq7Yk2f9Ld5Rc8Wm6Np4Ks1Xy7QmT0bGZt3Vq7Yk2f9Ld5\n\
Rc8Wm6Np4Ks1Xy7QmT0bGZt3Vq7Yk2f9Ld5Rc8Wm6Np4Ks1Xy7QmT0bGZt3Vq7Yk2f9\n\
Ld5Rc8Wm6Np4Ks1Xy7QmT0bGZt3Vq7Yk2f9Ld5Rc8Wm6Np4Ks1Xy7QmT0bGZt3Vq7Yk\n\
2f9Ld5Rc8Wm6Np4Ks1Xy7QmT0bGZt3Vq7Yk2f9Ld5Rc8Wm6Np4Ks1Xy7QmT0bGZt3Vq\n\
7Yk2f9Ld5Rc8Wm6Np4Ks1Xy7QmT0bGZt3Vq7Yk2f9Ld5Rc8Wm6Np4Ks1Xy7QmT0bGZ\n\
t3Vq7Yk2f9Ld5Rc8Wm6Np4Ks1Xy7QmT0bGZt3Vq7Yk2f9Ld5Rc8Wm6Np4Ks1Xy7QmT\n\
--- 1sVJx0n4YQmT0bGZt3Vq7Yk2f9Ld5Rc8Wm6Np4Ks1Xy\n\
-----END AGE ENCRYPTED FILE-----\n";

/// Insert a minimal vault row so FK constraints pass in tests.
///
/// Goes through the public `save_vault` API; the vault id doubles as the name
/// so the row stays unique without a caller-supplied name.
#[cfg(feature = "integration")]
pub async fn seed_vault(storage: &impl VaultStore, vault_id: Uuid) -> Result<()> {
    let vault = test_vault_with(vault_id, &vault_id.to_string(), false, vec![]);
    storage.save_vault(&vault).await
}
