//! `genatp-cassette` — record / replay / self-heal Action Cassettes.
//!
//! **Phase 7** (Week 12–14): Record successful agent runs as signed,
//! versioned cassettes.
//!
//! **Phase 8** (Week 14–16): Replay cassettes deterministically with
//! zero LLM tokens; self-heal on DOM drift via scoped LLM patch.
//!
//! ## Cassette format
//!
//! ```text
//! ┌────────────────────────────────────────────────────────┐
//! │  zstd( canonical_json( Cassette ) )  ||  ed25519 sig  │
//! └────────────────────────────────────────────────────────┘
//! ```
//!
//! - **JSON (JCS)**: human-inspectable after `zstd -d`.
//! - **zstd**: 5–10× compression on repetitive JSON.
//! - **ed25519**: 64-byte signature, deterministic, fast.
//! - **Hash-chained steps**: tamper with one step → every subsequent
//!   hash breaks.
//!
//! ## Modules
//!
//! - [`canon`] — JCS canonical JSON serialization (RFC 8785).
//! - [`error`] — error types.
//! - [`record`] — convert a completed `EventLog` into a `Cassette`.
//! - [`replay`] — deterministic zero-LLM replay.
//! - [`router`] — cassette hit → replay before plan; miss → host plans.
//! - [`heal`] — scoped LLM patch when Click/Fill refs drift.
//! - [`schema`] — `Cassette`, `CassetteStep`, `CassetteHeader` structs.
//! - [`signing`] — ed25519 key management, sign, verify.
//! - [`storage`] — filesystem persistence, slug/version resolution.
//!
//! [`docs/DEVELOPMENT_PLAN.md`]: ../../../docs/DEVELOPMENT_PLAN.md

#![forbid(unsafe_code)]

pub mod canon;
pub mod error;
pub mod heal;
pub mod record;
pub mod replay;
pub mod router;
pub mod schema;
pub mod signing;
pub mod storage;

pub use error::{CassetteError, Result};
pub use heal::HealConfig;
pub use record::CassetteRecorder;
pub use replay::{CassetteReplayer, ReplayOutcome, ReplayStepResult, ReplayStepStatus};
pub use router::{RouteHit, RouteOptions, enforce_vault_capability, try_route};
pub use schema::{CASSETTE_EXT, Cassette, CassetteHeader, CassetteStep, SCHEMA_VERSION};
pub use signing::{ensure_keypair, public_key_from_hex, public_key_hex};
pub use storage::{cassette_path, cassettes_dir, list_slugs, list_versions, load, save};

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
