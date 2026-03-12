//! Port trait definitions (hexagonal architecture)
//!
//! These traits define the interfaces that adapters must implement.

pub mod crypto;
pub mod storage;

pub use crypto::Crypto;
pub use storage::Storage;
