//! Documented stubs for Google Gemini, Mistral, and vLLM.
//!
//! These exist so the orchestrator can typecheck against the full set of
//! provider names without committing partial implementations. Each returns
//! [`LlmError::NotImplemented`] on `complete`.
//!
//! Replace with a real adapter when the corresponding milestone lands
//! (planned for v0.4 — see `docs/DEVELOPMENT_PLAN.md` Phase 3 backlog).

use async_trait::async_trait;

use crate::error::{LlmError, Result};
use crate::provider::LlmProvider;
use crate::types::{CompletionRequest, CompletionResponse};

macro_rules! stub_provider {
    ($Name:ident, $name:literal) => {
        /// Placeholder for the corresponding upstream provider.
        ///
        /// All `complete` calls return [`LlmError::NotImplemented`]. Use
        /// during prototyping; replace before relying on this provider in
        /// production.
        #[derive(Debug, Default, Clone, Copy)]
        pub struct $Name;

        #[async_trait]
        impl LlmProvider for $Name {
            fn name(&self) -> &'static str {
                $name
            }

            async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
                Err(LlmError::NotImplemented($name))
            }
        }
    };
}

stub_provider!(GoogleProvider, "google");
stub_provider!(MistralProvider, "mistral");
stub_provider!(VllmProvider, "vllm");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Message;

    #[tokio::test(flavor = "current_thread")]
    async fn each_stub_returns_not_implemented() {
        for p in [
            GoogleProvider.name(),
            MistralProvider.name(),
            VllmProvider.name(),
        ] {
            assert!(["google", "mistral", "vllm"].contains(&p));
        }

        let req = CompletionRequest::chat("m", vec![Message::user("hi")]);
        for r in [
            GoogleProvider.complete(req.clone()).await,
            MistralProvider.complete(req.clone()).await,
            VllmProvider.complete(req.clone()).await,
        ] {
            assert!(matches!(r, Err(LlmError::NotImplemented(_))));
        }
    }
}
