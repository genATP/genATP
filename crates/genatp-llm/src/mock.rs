//! Scripted `LlmProvider` for tests.
//!
//! The orchestrator's integration tests need a deterministic provider that
//! emits exactly the tool calls / assistant text we want, in order. The
//! [`MockProvider`] is the simplest thing that does that: a queue of
//! pre-baked [`CompletionResponse`]s that successive `complete` calls pop
//! off the front.
//!
//! When the queue is empty the provider returns
//! [`LlmError::Misconfigured`] so tests don't silently pass after running
//! out of scripted turns.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::error::{LlmError, Result};
use crate::provider::LlmProvider;
use crate::types::{CompletionRequest, CompletionResponse, FinishReason, Message, Role, Usage};

/// In-memory `LlmProvider` that returns scripted responses.
#[derive(Debug, Default)]
pub struct MockProvider {
    /// Queue of responses to return in order on `complete`.
    queue: Mutex<Vec<CompletionResponse>>,
    /// Record of every request the provider saw, in order.
    seen: Mutex<Vec<CompletionRequest>>,
}

impl MockProvider {
    /// Build a fresh provider with the given response script.
    #[must_use]
    pub fn new(responses: Vec<CompletionResponse>) -> Self {
        Self {
            queue: Mutex::new(responses),
            seen: Mutex::new(Vec::new()),
        }
    }

    /// Build a provider that returns a single plain-text reply.
    #[must_use]
    pub fn with_reply(text: impl Into<String>) -> Self {
        Self::new(vec![CompletionResponse {
            message: Message::new(Role::Assistant, text),
            finish_reason: FinishReason::Stop,
            usage: Usage::default(),
            provider: "mock".into(),
            model: "mock".into(),
        }])
    }

    /// Append a response to the script.
    pub fn push(&self, response: CompletionResponse) {
        self.queue.lock().expect("mock queue lock").push(response);
    }

    /// All requests seen so far (cloned snapshot).
    #[must_use]
    pub fn requests(&self) -> Vec<CompletionRequest> {
        self.seen.lock().expect("mock seen lock").clone()
    }

    /// Number of requests seen.
    #[must_use]
    pub fn call_count(&self) -> usize {
        self.seen.lock().expect("mock seen lock").len()
    }

    /// Number of responses still queued.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.queue.lock().expect("mock queue lock").len()
    }
}

#[async_trait]
impl LlmProvider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        self.seen.lock().expect("mock seen lock").push(request);
        let mut q = self.queue.lock().expect("mock queue lock");
        if q.is_empty() {
            return Err(LlmError::Misconfigured(
                "mock provider exhausted; add more scripted responses".into(),
            ));
        }
        Ok(q.remove(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CompletionRequest;

    #[tokio::test(flavor = "current_thread")]
    async fn returns_scripted_responses_in_order() {
        let provider = MockProvider::new(vec![
            CompletionResponse {
                message: Message::assistant("first"),
                finish_reason: FinishReason::Stop,
                usage: Usage::default(),
                provider: "mock".into(),
                model: "mock".into(),
            },
            CompletionResponse {
                message: Message::assistant("second"),
                finish_reason: FinishReason::Stop,
                usage: Usage::default(),
                provider: "mock".into(),
                model: "mock".into(),
            },
        ]);

        let r1 = provider
            .complete(CompletionRequest::chat("m", vec![Message::user("a")]))
            .await
            .unwrap();
        let r2 = provider
            .complete(CompletionRequest::chat("m", vec![Message::user("b")]))
            .await
            .unwrap();
        assert_eq!(r1.message.content, "first");
        assert_eq!(r2.message.content, "second");
        assert_eq!(provider.call_count(), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exhausted_provider_errors() {
        let provider = MockProvider::new(vec![]);
        let err = provider
            .complete(CompletionRequest::chat("m", vec![Message::user("x")]))
            .await
            .expect_err("must error");
        assert!(matches!(err, LlmError::Misconfigured(_)));
    }
}
