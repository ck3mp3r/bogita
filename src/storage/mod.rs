//! Bogita Storage - SQLite adapter implementation
//!
//! This crate implements the Storage port trait using SQLite.

pub mod config;
pub mod sqlite;

#[cfg(test)]
mod sqlite_test;

// Re-export for convenience
pub use config::{default_config_dir, default_data_dir, default_db_path};
