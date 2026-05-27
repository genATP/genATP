//! Task description handed to the orchestrator.
//!
//! A `Task` is the **input** — what the agent should accomplish, in
//! plain English, plus the constraints under which it must operate.

use serde::{Deserialize, Serialize};

use crate::budget::{BudgetLimits, Pricing};
use crate::retry::RetryPolicy;

/// One task to run through the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier (caller-supplied). Used as the event-log filename.
    pub id: String,
    /// Free-form goal description sent to the model.
    pub goal: String,
    /// URL the agent should start on.
    pub start_url: String,
    /// LLM model identifier to pass through to the provider.
    pub model: String,
    /// System prompt prepended to every conversation.
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Maximum agent steps before the orchestrator gives up (independent
    /// of token / wall-clock limits, useful for "I'm stuck" loops).
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
    /// Hard budget the agent must not exceed.
    pub budget: BudgetLimits,
    /// Per-model pricing for USD budget tracking. Use [`Pricing::FREE`]
    /// when running against local providers.
    pub pricing: Pricing,
    /// Retry behaviour on transient LLM / browser errors.
    #[serde(default)]
    pub retry: RetryPolicy,
}

fn default_max_steps() -> u32 {
    30
}

impl Task {
    /// Convenience: minimal task with sensible defaults.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        goal: impl Into<String>,
        start_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            goal: goal.into(),
            start_url: start_url.into(),
            model: model.into(),
            system_prompt: None,
            max_steps: default_max_steps(),
            budget: BudgetLimits::default(),
            pricing: Pricing::FREE,
            retry: RetryPolicy::default(),
        }
    }
}
