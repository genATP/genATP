//! Error types for the ATP Spec crate.
//!
//! Three levels of error:
//! - **ParseError** — YAML syntax or schema violation.
//! - **ValidationError** — semantic rule violation (e.g. missing name, invalid step).
//! - **RunError** — execution failure (browser, timeout, assertion).

use std::path::PathBuf;

use thiserror::Error;

/// Crate-wide result alias.
pub type Result<T> = std::result::Result<T, AtpSpecError>;

/// Unified error type for all ATP Spec operations.
#[derive(Debug, Error)]
pub enum AtpSpecError {
    // ── Parse / schema errors ─────────────────────────────────────
    /// YAML file could not be read.
    #[error("cannot read spec file `{path}`: {source}")]
    ReadError {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// YAML syntax error (malformed file).
    #[error("YAML parse error in `{path}`: {detail}")]
    YamlParse {
        /// Path with the error.
        path: PathBuf,
        /// Human-readable parse diagnostic.
        detail: String,
    },

    /// Structural validation failure (missing required fields, unknown keys, etc.).
    #[error("spec validation error: {0}")]
    Validation(String),

    /// Unsupported schema version.
    #[error("unsupported ATP spec schema version {found}; supported: {supported}")]
    UnsupportedVersion {
        /// Version found in the file.
        found: u32,
        /// Highest version this binary understands.
        supported: u32,
    },

    // ── Run-time errors ───────────────────────────────────────────
    /// Browser operation failed (navigation, click, fill, etc.).
    #[error("browser error at step {step}: {detail}")]
    Browser {
        /// 0-based step index.
        step: usize,
        /// Step name for context.
        step_name: String,
        /// Human-readable detail.
        detail: String,
    },

    /// An assertion failed.
    #[error("assertion failed at step {step}: {detail}")]
    Assertion {
        /// 0-based step index.
        step: usize,
        /// Step name for context.
        step_name: String,
        /// Description of what was expected vs actual.
        detail: String,
    },

    /// Spec timed out (wall clock or step count).
    #[error("spec timed out after {elapsed}")]
    Timeout {
        /// Human-readable duration.
        elapsed: String,
    },

    /// Runner encountered an internal error (shouldn't happen in normal use).
    #[error("runner internal error: {0}")]
    Internal(String),

    /// Cassette error (loading, replaying, recording).
    #[error("cassette error: {0}")]
    Cassette(String),

    /// Report serialization error (JUnit XML, JSON).
    #[error("report error: {0}")]
    Report(String),

    /// Input/output error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl AtpSpecError {
    /// Whether the error is recoverable by retrying.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Browser { .. } | Self::Timeout { .. } | Self::Cassette(_)
        )
    }
}
