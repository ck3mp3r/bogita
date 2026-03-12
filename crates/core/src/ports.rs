//! Port trait definitions (hexagonal architecture)
//!
//! These traits define the interfaces that adapters must implement.

pub mod storage;

pub use storage::Storage;
