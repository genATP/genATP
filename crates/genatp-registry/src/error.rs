//! Error types for the cassette registry.

use std::path::PathBuf;

/// All errors surfaced by `genatp-registry`.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("i/o error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("zstd: {0}")]
    Zstd(String),
    #[error("cassette: {0}")]
    Cassette(#[from] genatp_cassette::CassetteError),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid version constraint: {0}")]
    InvalidConstraint(String),
    #[error("publisher not found: {0}")]
    PublisherNotFound(String),
    #[error("storage: {0}")]
    Storage(String),
    #[error("signature: {0}")]
    Signature(String),
    #[error("trust: unsigned cassettes are blocked")]
    UnsignedBlocked,
    #[error("trust: self-signed cassette requires confirmation")]
    SelfSignedRequiresConfirm,
    #[error("config: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, RegistryError>;

impl From<std::io::Error> for RegistryError {
    fn from(source: std::io::Error) -> Self {
        Self::Io {
            path: PathBuf::new(),
            source,
        }
    }
}
