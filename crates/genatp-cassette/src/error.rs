//! Error types for the cassette crate.
//!
//! The error hierarchy mirrors the vault pattern: distinct variants for
//! each failure domain, with deliberately vague messages for security-
//! sensitive operations (signatures, key material).

use std::path::PathBuf;

use genatp_core::action::Action;

/// All errors surfaced by `genatp-cassette`.
#[derive(Debug, thiserror::Error)]
pub enum CassetteError {
    /// I/O failure (file read/write, directory creation).
    #[error("cassette io: {0}")]
    Io(#[from] std::io::Error),

    /// JSON (de)serialization failure.
    #[error("cassette serde: {0}")]
    Serde(#[from] serde_json::Error),

    /// Zstd compression or decompression failure.
    #[error("cassette zstd: {0}")]
    Zstd(String),

    /// Ed25519 signature verification failed. Deliberately vague — do
    /// not reveal *why* verification failed (wrong key, tampered data,
    /// truncated signature, …).
    #[error("cassette signature invalid")]
    SignatureInvalid,

    /// The signing key file could not be loaded or parsed.
    #[error("cassette signing key error: {0}")]
    SigningKey(String),

    /// Hash-chain integrity failure at the given step index.
    #[error("cassette hash-chain broken at step {step}")]
    HashChainBroken { step: u32 },

    /// Cassette file not found at the expected path.
    #[error("cassette not found: {path}")]
    NotFound { path: PathBuf },

    /// No cassettes exist for the given slug.
    #[error("no cassettes for slug `{slug}`")]
    SlugNotFound { slug: String },

    /// The cassette schema version is not supported.
    #[error("unsupported cassette schema version: {version}")]
    UnsupportedVersion { version: u32 },

    /// The event log does not contain a successful run (required for
    /// recording).
    #[error("cannot record: {reason}")]
    RecordingFailed { reason: String },

    /// Cassette storage directory could not be resolved.
    #[error("cassette storage: {0}")]
    Storage(String),

    /// Replay failed on a specific step.
    #[error("replay failed at step {step}: {reason} (action: {action:?})")]
    ReplayFailed {
        step: u32,
        action: Action,
        reason: String,
    },
}

pub type Result<T> = std::result::Result<T, CassetteError>;
