//! Error types for the perception engine.
//!
//! Public surface: [`PerceptionError`] and [`Result<T>`]. Each variant
//! captures enough context for callers (orchestrator, debugger, MCP server)
//! to make routing decisions without re-parsing strings.

use std::path::PathBuf;

/// Crate-wide alias.
pub type Result<T> = std::result::Result<T, PerceptionError>;

/// All errors produced by `genatp-perception`.
#[derive(Debug, thiserror::Error)]
pub enum PerceptionError {
    /// Raw a11y tree referenced a `parent_id` / `child_id` that does not
    /// exist in the node list.
    #[error("dangling node reference: {referrer} → {target}")]
    DanglingReference { referrer: String, target: String },

    /// Raw a11y tree contains a cycle in parent/child relationships.
    #[error("cycle detected in a11y tree starting at {0}")]
    CycleDetected(String),

    /// Tried to apply a [`Patch`](crate::Patch) op to a snapshot that does
    /// not contain the targeted ref.
    ///
    /// This signals the caller's previous-snapshot is out of date — typically
    /// because they started a new page session without a fresh `compress`.
    #[error("patch targets unknown ref `{0}`")]
    UnknownRef(String),

    /// Tried to add a node whose parent ref does not exist in the snapshot
    /// being patched.
    #[error("patch adds node under unknown parent ref `{0}`")]
    UnknownParent(String),

    /// SQLite store error (open, prepare, exec).
    #[error("snapshot store: {0}")]
    Store(#[from] rusqlite::Error),

    /// Filesystem error opening / creating the store directory.
    #[error("snapshot store io ({path}): {source}")]
    StoreIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// JSON serialise / deserialise during compression or hashing.
    #[error("snapshot serde: {0}")]
    Serde(#[from] serde_json::Error),

    /// zstd compression / decompression failed.
    #[error("zstd codec: {0}")]
    Zstd(std::io::Error),
}
