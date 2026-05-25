//! `genatp-perception` — token-optimised page snapshots, ref compression,
//! diff engine, content-addressed storage, and a vision adapter scaffold.
//!
//! ## Pipeline
//!
//! ```text
//!  ┌──────────────┐   strip   ┌──────────────┐   refs   ┌──────────────────┐   diff    ┌────────┐
//!  │  RawTree     │ ────────► │  RawTree'    │ ───────► │  CompressedSnap  │ ────────► │ Patch  │
//!  │  (CDP a11y)  │           │  (decorative │          │  (@e1, @e2 …,    │           │  Ops   │
//!  │              │           │   stripped)  │          │   pre-order DFS) │           │        │
//!  └──────────────┘           └──────────────┘          └──────────────────┘           └────────┘
//!                                                              │
//!                                                              ▼
//!                                                        ┌────────────┐
//!                                                        │  CAS store │ (BLAKE3, zstd, SQLite)
//!                                                        └────────────┘
//! ```
//!
//! ## Public API at a glance
//!
//! | Function                                | Purpose                                |
//! | --------------------------------------- | -------------------------------------- |
//! | [`compress`]                            | Strip + ref + project a `RawTree`      |
//! | [`diff`]                                | Compute a [`Patch`] between snapshots  |
//! | [`apply`]                               | Apply a [`Patch`] to a snapshot        |
//! | [`SnapshotStore`]                       | BLAKE3-keyed SQLite/zstd CAS           |
//! | [`TokenEstimator`]                      | Per-model chars/4 token estimator      |
//! | [`VisionProvider`] / [`NullVisionProvider`] | Phase 12 vision-adapter scaffold   |
//!
//! Every type that crosses an LLM / cassette boundary derives `Serialize` +
//! `Deserialize`. The compressed snapshot is **canonical**: byte-equality on
//! the JSON form is a sound semantic equality check.
//!
//! ## Stability contract
//!
//! - **Refs**: stable across snapshots in a session if you thread one
//!   [`RefMap`] through successive [`compress`] calls. See
//!   [`refs`] module docs.
//! - **Diffs**: `apply(diff(a, b), a) == b` for any canonical pair.
//!   Property-tested in `tests/diff_invertibility.rs`.
//! - **Hashes**: changing the JSON layout (field order, default-skipping)
//!   rotates the BLAKE3 hash — treat such changes as schema-breaking.

#![forbid(unsafe_code)]
#![allow(
    clippy::module_name_repetitions,
    clippy::missing_errors_doc, // documented per-fn already
    clippy::missing_panics_doc, // we never panic in public API
    clippy::cast_possible_truncation, // estimator math intentionally truncates
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value,
)]

pub mod compress;
pub mod diff;
pub mod error;
pub mod raw;
pub mod refs;
pub mod snapshot;
pub mod store;
pub mod strip;
pub mod tokens;
pub mod vision;

pub use compress::{CompressOptions, compress};
pub use diff::{Op, Patch, apply, diff};
pub use error::{PerceptionError, Result};
pub use raw::{RawNode, RawProperty, RawTree};
pub use refs::RefMap;
pub use snapshot::{CompressedNode, CompressedSnapshot, NodeFlags, Ref};
pub use store::{ContentHash, SnapshotStore};
pub use strip::{StripPolicy, strip};
pub use tokens::{ModelKind, TokenEstimator};
pub use vision::{
    BoundingBox, NullVisionProvider, VisionAction, VisionError, VisionProvider, VisionResponse,
};

/// Returns this crate's package version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::version;

    #[test]
    fn version_is_non_empty() {
        assert!(!version().is_empty());
    }
}
