//! Provider trait — the contract every adapter implements.
//!
//! Keeping the trait narrow keeps adapter code small. Streaming is a
//! deliberate non-goal for Phase 3 (it complicates the orchestrator and is
//! only needed for chat UIs we don't ship yet). The trait will gain a
//! `complete_stream` variant in Phase 12 along with vision providers.

use async_trait::async_trait;

use crate::error::Result;
use crate::types::{CompletionRequest, CompletionResponse};

/// All `genatp-llm` adapters implement this trait.
///
/// Adapters are typically `Arc<dyn LlmProvider>` so the orchestrator can
/// fan out concurrent requests without holding the trait bound everywhere.
#[async_trait]
pub trait LlmProvider: Send + Sync + std::fmt::Debug {
    /// Lower-cased provider identifier (`"anthropic"`, `"openai"`, …).
    fn name(&self) -> &'static str;

    /// Run a single completion. Adapters are responsible for translating
    /// the provider-neutral request into their wire format and back.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
}
