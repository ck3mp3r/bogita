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

    /// Config errors
    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    /// Session errors
    #[error("session error: {0}")]
    Session(#[from] SessionError),

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

    #[error("keychain error: {0}")]
    KeychainError(String),
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
pub enum ConfigError {
    #[error("config file not found")]
    NotFound,

    #[error("config parse failed: {0}")]
    ParseFailed(String),

    #[error("config write failed: {0}")]
    WriteFailed(String),
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("invalid entry name: {0}")]
    InvalidName(String),

    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("empty password")]
    EmptyPassword,

    #[error("invalid OTP secret: {0}")]
    InvalidOtpSecret(String),

    #[error("invalid OTP URI: {0}")]
    InvalidOtpUri(String),
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("vault is locked — run `bogita unlock`")]
    Locked,

    #[error("keychain unavailable: {0}")]
    KeychainUnavailable(String),
}
