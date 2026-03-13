//! Error types for bogita-core

use thiserror::Error;

/// Core result type
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type
#[derive(Debug, Error)]
pub enum Error {
    /// Database errors
    #[error("database error: {0}")]
    Database(#[from] DbError),

    /// Cryptography errors
    #[error("encryption error: {0}")]
    Crypto(#[from] CryptoError),

    /// Sync errors
    #[error("sync error: {0}")]
    Sync(#[from] SyncError),

    /// Validation errors
    #[error("validation error: {0}")]
    Validation(#[from] ValidationError),

    /// I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("entry not found: {0}")]
    EntryNotFound(uuid::Uuid),

    #[error("vault not found: {0}")]
    VaultNotFound(uuid::Uuid),

    #[error("duplicate entry name: {0}")]
    DuplicateName(String),

    #[error("database connection failed: {0}")]
    ConnectionFailed(String),

    #[error("database migration failed: {0}")]
    MigrationFailed(String),

    #[error("corrupted data in database")]
    CorruptedData,

    #[error("database query error: {0}")]
    Query(String),
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("encryption failed")]
    EncryptionFailed,

    #[error("decryption failed")]
    DecryptionFailed,

    #[error("invalid recipient: {0}")]
    InvalidRecipient(String),

    #[error("invalid identity: {0}")]
    InvalidIdentity(String),
}

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("git error: {0}")]
    Git(String),

    #[error("remote unreachable")]
    RemoteUnreachable,

    #[error("authentication failed")]
    AuthenticationFailed,
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("invalid entry name: {0}")]
    InvalidName(String),

    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("empty password")]
    EmptyPassword,

    #[error("invalid OTP secret")]
    InvalidOtpSecret,
}
