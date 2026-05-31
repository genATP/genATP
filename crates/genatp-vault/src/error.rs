//! Vault error type.
//!
//! All fallible vault operations return [`Result<T>`]. Variants are
//! deliberately coarse — callers that need to distinguish causes do so
//! via the [`VaultError`] discriminator, but most user-facing surfaces
//! collapse them into a single "vault failed" message. We never include
//! key material or plaintext in error messages.

use thiserror::Error;

/// Crate-wide result alias.
pub type Result<T> = std::result::Result<T, VaultError>;

/// Vault failure modes.
///
/// Several variants intentionally erase upstream error detail so that
/// stack traces / logs cannot leak ciphertext, IVs, or keychain
/// contents. The crate's tests explicitly assert that no plaintext
/// material flows into these messages.
#[derive(Debug, Error)]
pub enum VaultError {
    /// The vault file does not exist or is inaccessible.
    #[error("vault file not found at {0}")]
    NotFound(std::path::PathBuf),

    /// The OS keychain rejected access (user cancelled, locked, etc.).
    /// The wrapped string is sanitized — the raw OS error is dropped.
    #[error("OS keychain unavailable: {0}")]
    Keychain(String),

    /// The keychain is reachable but no master KEK is provisioned for
    /// this vault. Call [`crate::Vault::initialize`] first.
    #[error("vault is not initialized — run `genatp vault init`")]
    NotInitialized,

    /// AES-GCM authentication failure. Either the ciphertext was
    /// tampered with, or the wrong DEK was used. Never indicates *which*
    /// — that distinction would itself be a side-channel.
    #[error("decryption failed (wrong key or corrupted vault)")]
    Decrypt,

    /// SQLite error during schema setup, query, or migration.
    #[error("database error: {0}")]
    Database(String),

    /// On-disk schema version is newer than this binary supports.
    #[error("vault schema version {found} is newer than supported {supported}")]
    SchemaTooNew {
        /// Version found on disk.
        found: i64,
        /// Highest version this binary understands.
        supported: i64,
    },

    /// Domain rejected by canonicalization (empty, contains scheme, etc.).
    #[error("invalid domain `{0}`")]
    InvalidDomain(String),

    /// Capability check failed — the caller did not declare the
    /// requested domain in its grant list.
    #[error("capability denied for domain `{0}`")]
    CapabilityDenied(String),

    /// Filesystem I/O failure (vault file read/write, parent dir missing).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialisation / deserialisation of an entry payload.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// The supplied passphrase was empty or below the minimum length.
    #[error("passphrase rejected: {0}")]
    BadPassphrase(&'static str),
}

impl From<rusqlite::Error> for VaultError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value.to_string())
    }
}

impl From<keyring::Error> for VaultError {
    fn from(value: keyring::Error) -> Self {
        Self::Keychain(value.to_string())
    }
}
