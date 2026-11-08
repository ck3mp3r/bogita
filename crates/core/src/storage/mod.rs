//! Bogita Storage - SQLite adapter implementation
//!
//! This crate implements the Storage port trait using SQLite.

pub mod config;
pub mod entry_store;
pub mod identity;
pub mod keychain;
pub mod mapper;
pub mod sqlite;
pub mod vault_store;

// Re-export for convenience
pub use config::{default_db_path, default_identity_path, AppConfig};
pub use keychain::KeychainAdapter;
