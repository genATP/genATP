//! `genatp-llm` — provider-agnostic LLM trait and adapters.
//!
//! Implementation phase: **Phase 3**.
//!
//! ## Architecture
//!
//! The crate is shaped around three layers:
//!
//! 1. **Types** — [`types`] defines `Message`, `Role`, `ToolDefinition`,
//!    `ToolCall`, `Usage`, `CompletionRequest`, `CompletionResponse`,
//!    `FinishReason`, `SamplingParams`. These are provider-neutral.
//! 2. **Trait** — [`provider::LlmProvider`] is the single contract every
//!    adapter implements.
//! 3. **Adapters** — [`providers`] modules hold the real implementations
//!    plus [`mock::MockProvider`] for tests and [`vcr::VcrProvider`] for
//!    record/replay CI.
//!
//! ## Production adapters in Phase 3
//!
//! - Anthropic ([`providers::anthropic::AnthropicProvider`])
//! - OpenAI ([`providers::openai::OpenAiProvider`])
//! - Ollama ([`providers::ollama::OllamaProvider`])
//!
//! Google, Mistral, and vLLM ship as documented placeholders
//! ([`providers::placeholders`]) returning [`LlmError::NotImplemented`].
//! They land in v0.4 once we wire up VCR fixtures for each.
//!
//! ## VCR — record once, replay always
//!
//! CI must never depend on live API keys. [`vcr::VcrProvider`] decorates a
//! real adapter, persists every (request, response) pair to a JSON
//! cassette keyed by a stable BLAKE3 hash of the canonical request JSON,
//! and replays from disk on subsequent runs.
//!
//! See `tests/vcr_round_trip.rs` for the canonical usage pattern.
//!
//! [`LlmError::NotImplemented`]: error::LlmError::NotImplemented

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]

pub mod error;
pub mod mock;
pub mod provider;
pub mod providers;
pub mod types;
pub mod vcr;

pub use error::{LlmError, Result};
pub use mock::MockProvider;
pub use provider::LlmProvider;
pub use types::{
    CompletionRequest, CompletionResponse, FinishReason, Message, Role, SamplingParams, ToolCall,
    ToolDefinition, Usage,
};
pub use vcr::{VcrMode, VcrProvider};

/// Returns this crate's package version (compile-time `CARGO_PKG_VERSION`).
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
