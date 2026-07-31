//! Age key types

use age::x25519;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Wrapper around age::x25519::Recipient (public key)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgeRecipient(x25519::Recipient);

impl AgeRecipient {
    pub fn inner(&self) -> &x25519::Recipient {
        &self.0
    }
}

impl FromStr for AgeRecipient {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<x25519::Recipient>()
            .map(AgeRecipient)
            .map_err(|e| format!("invalid age recipient: {}", e))
    }
}

impl fmt::Display for AgeRecipient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for AgeRecipient {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for AgeRecipient {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Wrapper around age::x25519::Identity (private key)
/// Note: x25519::Identity already handles zeroization internally
#[derive(Clone)]
pub struct AgeIdentity(x25519::Identity);

impl fmt::Debug for AgeIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AgeIdentity").field(&"<redacted>").finish()
    }
}

impl AgeIdentity {
    pub fn generate() -> Self {
        Self(x25519::Identity::generate())
    }

    pub fn to_recipient(&self) -> AgeRecipient {
        AgeRecipient(self.0.to_public())
    }

    pub fn to_secret_string(&self) -> SecretString {
        self.0.to_string()
    }

    pub fn inner(&self) -> &x25519::Identity {
        &self.0
    }
}

impl FromStr for AgeIdentity {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<x25519::Identity>()
            .map(AgeIdentity)
            .map_err(|e| format!("invalid age identity: {}", e))
    }
}
