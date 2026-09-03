//! Task description handed to the orchestrator.
//!
//! A `Task` is the **input** — what the agent should accomplish, in
//! plain English, plus the constraints under which it must operate.

use serde::{Deserialize, Serialize};

use crate::budget::{BudgetLimits, Pricing};
use crate::retry::RetryPolicy;

/// Default max conversation/context tokens before truncation (NIM-safe).
pub const DEFAULT_MAX_CONTEXT_TOKENS: u32 = 120_000;

/// Default hard token budget for SDK/CLI launches (0 still means unlimited).
pub const DEFAULT_MAX_TOKENS: u32 = 100_000;

/// Default wall-clock budget for SDK/CLI launches.
pub const DEFAULT_MAX_WALL_SECS: u64 = 600;

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
    /// When true, successful runs should be persisted as a cassette by the
    /// host (CLI/SDK) using [`Self::record_slug`].
    #[serde(default)]
    pub record: bool,
    /// Cassette slug when [`Self::record`] is true.
    #[serde(default)]
    pub record_slug: Option<String>,
    /// Soft ceiling on conversation size sent to the LLM (approx tokens).
    /// Prevents NIM/OpenAI context overflows. `0` disables truncation.
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: u32,
    /// When true, skip cassette router (always plan with LLM).
    #[serde(default)]
    pub skip_cassette: bool,
    /// Force a specific cassette slug for router lookup.
    #[serde(default)]
    pub cassette_slug: Option<String>,
}

fn default_max_steps() -> u32 {
    30
}

fn default_max_context_tokens() -> u32 {
    DEFAULT_MAX_CONTEXT_TOKENS
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
            budget: BudgetLimits::sdk_defaults(),
            pricing: Pricing::FREE,
            retry: RetryPolicy::default(),
            record: false,
            record_slug: None,
            max_context_tokens: DEFAULT_MAX_CONTEXT_TOKENS,
            skip_cassette: false,
            cassette_slug: None,
        }
    }
}
