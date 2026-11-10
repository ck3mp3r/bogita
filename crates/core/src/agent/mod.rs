//! SSH agent daemon — RFC 9987 serve loop over a gated unix socket.
//!
//! The daemon answers REQUEST_IDENTITIES(11) and SIGN_REQUEST(13) from in-memory
//! key material and refuses every other protocol message. It never reads stdin,
//! never writes to the vault, and never prompts.

pub mod client;
pub mod control;
pub mod error;
pub mod keyring;
pub mod server;
pub mod signals;
pub mod socket;
pub mod watcher;

pub use client::ServedKey;
pub use error::SshAgentError;
pub use keyring::{KeyRing, KeyRingEntry, SharedKeyRing};
pub use server::{serve, serve_with_signals, BogitaSession};
pub use socket::{default_socket_path, SocketGuard};
pub use watcher::{LockEvent, LockMonitor, LockWatcher, UnlockSource};
