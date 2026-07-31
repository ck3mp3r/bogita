//! Domain models and types

pub mod entity;
pub mod key;
pub mod preset;
pub mod sync;
pub mod vault;

#[cfg(test)]
mod preset_test;

// Re-export types from dependencies for convenience
pub use age::x25519;
pub use secrecy::{ExposeSecret, SecretString};
pub use uuid::Uuid;

// Re-export all domain types for backward compatibility
pub use entity::{Entry, EntryType, Field, FieldType, FieldValue};
pub use key::{AgeIdentity, AgeRecipient};
pub use sync::{Change, Operation, PushResult, SyncMetadata, SyncType};
pub use vault::{AwsConfig, GcpConfig, GitConfig, SyncTarget, Vault};
