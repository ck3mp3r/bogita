//! Error types for bogita-core

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Not implemented")]
    NotImplemented,
}
