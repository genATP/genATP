//! Error types surfaced by the orchestrator.
//!
//! Errors are explicitly disjoint from `genatp-browser::Error` and
//! `genatp_llm::LlmError`. The orchestrator wraps both via `From` impls so
//! callers see a single `CoreError`.

use std::time::Duration;

use genatp_llm::LlmError;

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Budget exhausted — soft or hard depending on the variant carried.
    #[error("budget: {0}")]
    Budget(#[from] crate::budget::BudgetError),

    /// LLM provider failure (forwarded unchanged for retry classification).
    #[error("llm: {0}")]
    Llm(#[from] LlmError),

    /// Browser-driver failure.
    #[error("browser: {0}")]
    Browser(String),

    /// Action dispatcher rejected an LLM-emitted tool call.
    #[error("invalid action `{name}`: {reason}")]
    InvalidAction { name: String, reason: String },

    /// Orchestrator hit its hard step limit without the agent finishing.
    #[error("step limit reached ({0} steps)")]
    StepLimit(u32),

    /// Wall-clock deadline hit.
    #[error("deadline reached after {0:?}")]
    Deadline(Duration),

    /// Anything to do with the event log (I/O, encoding, …).
    #[error("event log: {0}")]
    EventLog(String),

    /// Configuration bug (missing field, contradictory limits, …).
    #[error("config: {0}")]
    Config(String),

    /// Operator interrupted the run (Ctrl+C / cancel flag).
    #[error("interrupted: {0}")]
    Interrupted(String),

    /// Generic I/O.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serde failure (event log persistence, …).
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}

impl From<genatp_browser::BrowserError> for CoreError {
    fn from(value: genatp_browser::BrowserError) -> Self {
        Self::Browser(value.to_string())
    }
}

impl CoreError {
    /// Whether the orchestrator should consider this failure retryable.
    /// Forwards to underlying classifiers for `Llm` / `Browser`.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Llm(e) => e.is_retryable(),
            Self::Browser(_) => true,
            Self::Io(_) => true,
            _ => false,
        }
    }
}
