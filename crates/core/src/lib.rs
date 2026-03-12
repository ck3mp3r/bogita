//! Bogita Core - Domain logic and port definitions
//!
//! This crate contains the core domain logic and trait definitions (ports)
//! for the Bogita password manager. No external adapter dependencies.

pub mod domain;
pub mod error;
pub mod ports;

#[cfg(test)]
mod domain_test;
