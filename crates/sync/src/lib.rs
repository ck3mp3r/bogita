//! Bogita Sync - Git sync adapter implementation
//!
//! This crate implements the SyncPort trait using Git.

#[cfg(feature = "git")]
pub mod git;

pub mod implementation;
