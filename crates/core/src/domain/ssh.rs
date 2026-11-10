//! SSH key material — validation, keygen, and field mapping.
//!
//! Storage format v1 is an unencrypted OpenSSH private key PEM (the standard
//! armor block without passphrase encryption) plus its wire public key line.
//! Passphrase-protected PEMs are rejected: the private PEM rides the existing
//! per-field encryption path, so a passphrase layer would only add a second
//! secret to manage.

use crate::domain::{Field, FieldType, FieldValue};
use crate::error::{Result, ValidationError};
use ssh_key::rand_core::OsRng;
use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey};
use uuid::Uuid;
use zeroize::Zeroizing;

// region:    --- Types

/// Key of the plaintext boolean field that gates agent serving.
pub const AGENT_SERVABLE_KEY: &str = "agent_servable";

/// Field keys used when the material builds its own fields.
const PRIVATE_KEY_FIELD: &str = "private_key";
const PUBLIC_KEY_FIELD: &str = "public_key";
const KEY_TYPE_FIELD: &str = "key_type";
const COMMENT_FIELD: &str = "comment";

/// Validated SSH key material: one private PEM and everything derived from it.
///
/// The PEM buffer is zeroized on drop. Field mapping copies the PEM into
/// [`FieldValue::Hidden`] so it can be persisted by the encrypted field path.
pub struct SshKeyMaterial {
    private_pem: Zeroizing<String>,
    public_line: String,
    key_type: String,
    algorithm: Algorithm,
    comment: String,
    fingerprint: String,
}

// endregion: --- Types

// region:    --- Methods

impl SshKeyMaterial {
    /// Parse an unencrypted OpenSSH private key PEM.
    ///
    /// Returns `ValidationError::InvalidSshKey` when the PEM does not parse and
    /// `ValidationError::PassphraseProtectedKey` when it is passphrase-armored.
    pub fn validate(pem: impl Into<String>) -> Result<Self> {
        let pem = Zeroizing::new(pem.into());
        let key = PrivateKey::from_openssh(pem.as_bytes())
            .map_err(|e| ValidationError::InvalidSshKey(e.to_string()))?;

        if key.is_encrypted() {
            return Err(ValidationError::PassphraseProtectedKey.into());
        }

        Self::from_private_key(&key)
    }

    /// Generate a fresh Ed25519 key pair with the given comment.
    pub fn keygen_ed25519(comment: impl Into<String>) -> Result<Self> {
        let key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519)
            .map_err(|e| ValidationError::SshKeygen(e.to_string()))?;
        let key = PrivateKey::new(key.key_data().clone(), comment)
            .map_err(|e| ValidationError::SshKeygen(e.to_string()))?;

        Self::from_private_key(&key)
    }

    fn from_private_key(key: &PrivateKey) -> Result<Self> {
        let private_pem = key
            .to_openssh(LineEnding::LF)
            .map_err(|e| ValidationError::SshKeygen(e.to_string()))?;
        let public_line = key
            .public_key()
            .to_openssh()
            .map_err(|e| ValidationError::SshKeygen(e.to_string()))?;

        Ok(Self {
            private_pem,
            public_line,
            key_type: key_type_name(key.algorithm()),
            algorithm: key.algorithm(),
            comment: key.comment().to_string(),
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
        })
    }

    /// Private key PEM — unencrypted OpenSSH armor block.
    pub fn private_pem(&self) -> &str {
        &self.private_pem
    }

    /// Public key line in wire format (`ssh-ed25519 <base64> <comment>`).
    pub fn public_line(&self) -> &str {
        &self.public_line
    }

    /// Key type label stored in the `SshKeyType` field.
    pub fn key_type(&self) -> &str {
        &self.key_type
    }

    /// Algorithm of the parsed key, as the OpenSSH armor declares it.
    pub fn algorithm(&self) -> Algorithm {
        self.algorithm.clone()
    }

    /// Key comment stored in the `SshComment` field.
    pub fn comment(&self) -> &str {
        &self.comment
    }

    /// SHA-256 fingerprint as printed by `ssh-keygen -l` (`SHA256:<base64>`).
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Build the four `Ssh*` fields for a new key entry.
    pub fn to_fields(&self) -> Vec<Field> {
        let mut fields = Vec::with_capacity(4);
        self.upsert_ssh_fields(&mut fields);
        fields.sort_by_key(|f| f.idx);
        fields
    }

    /// Replace the `Ssh*` field values in place, appending missing fields.
    ///
    /// Existing field ids, keys, and indexes are preserved so an entry keeps
    /// its identity across rotation.
    pub fn apply_to(&self, fields: &mut Vec<Field>) {
        self.upsert_ssh_fields(fields);
    }

    fn upsert_ssh_fields(&self, fields: &mut Vec<Field>) {
        upsert_field(
            fields,
            FieldType::SshPrivateKey,
            PRIVATE_KEY_FIELD,
            FieldValue::Hidden(self.private_pem.to_string()),
            true,
        );
        upsert_field(
            fields,
            FieldType::SshPublicKey,
            PUBLIC_KEY_FIELD,
            FieldValue::Text(self.public_line.clone()),
            false,
        );
        upsert_field(
            fields,
            FieldType::SshKeyType,
            KEY_TYPE_FIELD,
            FieldValue::Text(self.key_type.clone()),
            false,
        );
        upsert_field(
            fields,
            FieldType::SshComment,
            COMMENT_FIELD,
            FieldValue::Text(self.comment.clone()),
            false,
        );
    }
}

// endregion: --- Methods

// region:    --- Support

/// Build the plaintext boolean field that gates agent serving.
pub fn agent_servable_field(on: bool, idx: i32) -> Field {
    Field {
        id: Uuid::new_v4(),
        key: AGENT_SERVABLE_KEY.to_string(),
        value: FieldValue::Boolean(on),
        field_type: FieldType::Boolean,
        encrypted: false,
        idx,
    }
}

/// Set the value of the first field of `field_type`, or append a new field.
fn upsert_field(
    fields: &mut Vec<Field>,
    field_type: FieldType,
    key: &str,
    value: FieldValue,
    encrypted: bool,
) {
    match fields.iter().position(|f| f.field_type == field_type) {
        Some(position) => {
            fields[position].value = value;
            fields[position].encrypted = encrypted;
        }
        None => {
            let idx = fields.len() as i32;
            fields.push(Field {
                id: Uuid::new_v4(),
                key: key.to_string(),
                value,
                field_type,
                encrypted,
                idx,
            });
        }
    }
}

/// Human-readable key type label (the field stores `Ed25519`, not `ssh-ed25519`).
///
/// Shared with the agent: load-time and opt-in refusals name the algorithm with
/// the same label the entry field carries.
pub(crate) fn key_type_name(algorithm: Algorithm) -> String {
    match algorithm {
        Algorithm::Ed25519 => "Ed25519".to_string(),
        Algorithm::Rsa { .. } => "Rsa".to_string(),
        other => other.to_string(),
    }
}

// endregion: --- Support

// region:    --- Tests
#[cfg(test)]
#[path = "../../test/domain/ssh.rs"]
mod ssh_test;
// endregion: --- Tests
