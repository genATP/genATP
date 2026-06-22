//! `genatp-debugger` — time-travel debugger backend (event log replay + REST API).
//!
//! ## Modules
//!
//! - [`models`] — API response types for the REST endpoints.
//! - [`reader`] — event log reader (JSON-Lines files from disk).
//! - [`api`] — axum-based REST API server with timeline, snapshot, and diff endpoints.
//! - [`replay`] — replay engine that reconstructs orchestrator state at a given step.
//!
//! ## Quick start
//!
//! ```no_run
//! # use std::net::SocketAddr;
//! # use std::path::PathBuf;
//! # use genatp_debugger::api::{AppState, serve};
//! # async fn ex() -> Result<(), Box<dyn std::error::Error>> {
//! let state = AppState {
//!     runs_dir: PathBuf::from("/tmp/genatp/runs"),
//!     snapshot_store: None,
//! };
//! let addr: SocketAddr = ([127, 0, 0, 1], 3001).into();
//! serve(state, &addr).await?;
//! # Ok(())
//! # }
//! ```
//!
//! [`docs/DEVELOPMENT_PLAN.md`]: ../../../docs/DEVELOPMENT_PLAN.md

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

pub mod api;
pub mod models;
pub mod reader;
pub mod replay;

pub use api::{AppState, router, serve};
pub use models::{
    DiffOpView, DiffResponse, RunSummary, SnapshotResponse, StepDetail, TimelineEvent,
};
pub use reader::discover_runs;
pub use replay::{ReplaySnapshot, ReplayState, replay_to_step, replay_to_step_with_snapshot};

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
