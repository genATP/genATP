//! Shared types for LLM provider requests and responses.
//!
//! These types are **provider-neutral**: each adapter maps from these into
//! its wire format and back. The orchestrator (Phase 3) talks only to
//! these types so swapping providers does not ripple changes outward.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One conversational role. Tool-call replies use [`Self::Tool`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System prompt — sets persona / rules.
    System,
    /// End user.
    User,
    /// Model reply.
    Assistant,
    /// Output produced by a tool the model called.
    Tool,
}

/// One message in a chat history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// When `role = Tool`, the id of the tool call this message answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// When `role = Assistant`, any tool calls the model emitted. Adapters
    /// surface this for [`crate::LlmProvider::complete_with_tools`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

impl Message {
    /// Convenience constructor for plain text messages.
    #[must_use]
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    /// System message.
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content)
    }
    /// User message.
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content)
    }
    /// Assistant message.
    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(Role::Assistant, content)
    }
    /// Reply to a tool call.
    #[must_use]
    pub fn tool(content: impl Into<String>, tool_call_id: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: Vec::new(),
        }
    }
}

/// Description of one tool the model is allowed to call.
///
/// `parameters` is a JSON-Schema object. Adapters forward it verbatim — we
/// don't try to translate between schema dialects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

impl ToolDefinition {
    /// Construct a tool definition.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }
}

/// One tool invocation emitted by the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider-stable id; orchestrator returns the result tagged with this.
    pub id: String,
    /// Tool name (must match a [`ToolDefinition::name`] passed in the request).
    pub name: String,
    /// JSON object of arguments. Always an object, even if empty.
    pub arguments: serde_json::Value,
}

/// Sampling / decoding hyperparameters. Optional — adapters substitute
/// provider-sensible defaults for `None`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SamplingParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Stop strings — when emitted, generation halts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    /// Free-form provider-specific knobs (`top_k`, `presence_penalty`, …).
    /// Stored sorted for stable cassette keys.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Request envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Model identifier (`"claude-3-5-sonnet-20241022"`, `"gpt-4o-2024-08-06"`,
    /// `"llama3.1"`, …). Adapters pass it through unchanged.
    pub model: String,
    pub messages: Vec<Message>,
    /// Tools available for the model to call.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,
    #[serde(default)]
    pub sampling: SamplingParams,
}

impl CompletionRequest {
    /// Construct a chat-only request (no tools).
    #[must_use]
    pub fn chat(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            tools: Vec::new(),
            sampling: SamplingParams::default(),
        }
    }

    /// Add tools, returning the updated request.
    #[must_use]
    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    /// Set sampling params.
    #[must_use]
    pub fn with_sampling(mut self, sampling: SamplingParams) -> Self {
        self.sampling = sampling;
        self
    }
}

/// Token accounting reported by the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl Usage {
    /// Sum two usages, saturating on overflow. Used by the orchestrator to
    /// fold per-step usage into the per-task budget.
    #[must_use]
    pub fn saturating_add(self, other: Self) -> Self {
        Self {
            prompt_tokens: self.prompt_tokens.saturating_add(other.prompt_tokens),
            completion_tokens: self
                .completion_tokens
                .saturating_add(other.completion_tokens),
            total_tokens: self.total_tokens.saturating_add(other.total_tokens),
        }
    }
}

/// Why the model stopped generating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// Model reached a natural stop.
    Stop,
    /// `max_tokens` reached.
    Length,
    /// Model emitted tool calls — orchestrator should dispatch them.
    ToolCalls,
    /// Provider-specific safety filter triggered.
    ContentFilter,
    /// Anything else (provider-specific reasons we don't map).
    Other,
}

/// Provider response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// The assistant's reply (may be empty if `tool_calls` is non-empty).
    pub message: Message,
    pub finish_reason: FinishReason,
    pub usage: Usage,
    /// Lower-cased provider identifier (`"anthropic"`, `"openai"`, `"ollama"`,
    /// `"mock"`, `"vcr"`). Useful for tracing / billing attribution.
    pub provider: String,
    /// Exact model id the provider used (may differ from request — providers
    /// alias names).
    pub model: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_add_saturates_on_overflow() {
        let a = Usage {
            prompt_tokens: u32::MAX,
            ..Usage::default()
        };
        let b = Usage {
            prompt_tokens: 1,
            ..Usage::default()
        };
        let c = a.saturating_add(b);
        assert_eq!(c.prompt_tokens, u32::MAX);
    }

    #[test]
    fn message_constructors_set_role() {
        assert_eq!(Message::system("s").role, Role::System);
        assert_eq!(Message::user("u").role, Role::User);
        assert_eq!(Message::assistant("a").role, Role::Assistant);
        let t = Message::tool("ok", "call_1");
        assert_eq!(t.role, Role::Tool);
        assert_eq!(t.tool_call_id.as_deref(), Some("call_1"));
    }

    #[test]
    fn completion_request_with_tools_appends() {
        let req = CompletionRequest::chat("m", vec![Message::user("hi")]).with_tools(vec![
            ToolDefinition::new("click", "click an element", serde_json::json!({})),
        ]);
        assert_eq!(req.tools.len(), 1);
    }

    #[test]
    fn role_serializes_lowercase() {
        let m = Message::user("hi");
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains("\"role\":\"user\""));
    }
}
