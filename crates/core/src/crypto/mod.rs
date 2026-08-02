//! Bogita Crypto - Age encryption adapter implementation
//!
//! This crate implements the Crypto port trait using the age encryption library.

pub mod age;
pub mod passphrase;

// Re-export for convenience
pub use age::AgeCrypto;
