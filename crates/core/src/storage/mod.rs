//! Bogita Storage - SQLite adapter implementation
//!
//! This crate implements the Storage port trait using SQLite.

pub mod config;
pub mod entry_store;
pub mod identity;
pub mod mapper;
pub mod sqlite;
pub mod vault_store;

#[cfg(test)]
mod config_test;
#[cfg(test)]
mod identity_test;
#[cfg(test)]
mod sqlite_test;

// Re-export for convenience
pub use config::{
    default_config_dir, default_data_dir, default_db_path, default_identity_path, AppConfig,
};
pub use identity::{read_identity, write_identity};
