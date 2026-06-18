//! `genatp-core` — orchestrator that wires perception, the LLM, and the
//! browser into a single closed-loop agent.
//!
//! Implementation phase: **Phase 3**.
//!
//! ## Modules
//!
//! - [`action`] — typed actions the model can request, plus their
//!   JSON-Schema tool definitions.
//! - [`budget`] — soft/hard limit enforcement (tokens, wall time, USD).
//! - [`dispatcher`] — maps actions to a [`BrowserAdapter`].
//! - [`event_log`] — append-only, hash-chained run log (feeds the Phase 13
//!   debugger).
//! - [`orchestrator`] — the plan loop (snapshot → LLM → action → repeat).
//! - [`page_adapter`] — real [`BrowserAdapter`] backed by
//!   `genatp_browser::Page`.
//! - [`perception_bridge`] — convert `AccessibilityTree` → `RawTree`.
//! - [`retry`] — exponential backoff retry policy.
//! - [`task`] — `Task` description handed to [`orchestrator::run`].
//!
//! ## Quick start
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use genatp_core::{event_log::EventLog, orchestrator, page_adapter::PageAdapter, task::Task};
//! # use genatp_llm::MockProvider;
//! # async fn ex(page: genatp_browser::Page) -> Result<(), Box<dyn std::error::Error>> {
//! let task = Task::new("demo", "Click the primary button.", "https://example.com", "mock");
//! let provider = Arc::new(MockProvider::with_reply("done"));
//! let mut log = EventLog::in_memory();
//! use genatp_perception::PolicyEngine;
//! let outcome = orchestrator::run(orchestrator::OrchestratorConfig {
//!     task,
//!     browser: PageAdapter::new(page),
//!     provider,
//!     event_log: &mut log,
//!     snapshot_store: None,
//!     policy_engine: PolicyEngine::default(),
//!     vision_provider: None,
//! })
//! .await?;
//! # let _ = outcome;
//! # Ok(()) }
//! ```
//! [`BrowserAdapter`]: dispatcher::BrowserAdapter

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

pub mod action;
pub mod budget;
pub mod dispatcher;
pub mod error;
pub mod event_log;
pub mod orchestrator;
pub mod page_adapter;
pub mod perception_bridge;
pub mod retry;
pub mod task;

pub use action::{Action, ActionResult};
pub use budget::{BudgetCheck, BudgetError, BudgetGuard, BudgetLimits, BudgetSpend, Pricing};
pub use dispatcher::{ActionDispatcher, BrowserAdapter};
pub use error::{CoreError, Result};
pub use event_log::{Event, EventEntry, EventLog};
pub use orchestrator::{OrchestratorConfig, RunOutcome, run};
pub use page_adapter::PageAdapter;
pub use retry::RetryPolicy;
pub use task::Task;

/// Returns this crate's package version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_non_empty() {
        assert!(!version().is_empty());
    }
}
