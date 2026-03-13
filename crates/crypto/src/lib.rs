//! Bogita Crypto - Age encryption adapter implementation
//!
//! This crate implements the Crypto port trait using the age encryption library.

pub mod age;

// Re-export for convenience
pub use age::AgeCrypto;
