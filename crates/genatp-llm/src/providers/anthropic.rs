//! Anthropic Claude adapter (Messages API).
//!
//! Maps our provider-neutral [`CompletionRequest`] into Anthropic's
//! `/v1/messages` request, and the response back. Tool calls are translated
//! via Anthropic's `tool_use` blocks.
//!
//! Endpoint contract:
//! - URL: `<base_url>/v1/messages`
//! - Auth: `x-api-key` header
//! - API version: `anthropic-version: 2023-06-01` (frozen for stability)
//!
//! Reference: <https://docs.anthropic.com/en/api/messages>

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{LlmError, Result};
use crate::provider::LlmProvider;
use crate::types::{
    CompletionRequest, CompletionResponse, FinishReason, Message, Role, ToolCall, Usage,
};

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Adapter configuration.
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub base_url: String,
    pub http_timeout: Duration,
}

impl AnthropicConfig {
    /// Build config from `ANTHROPIC_API_KEY`. Falls back to `Misconfigured`
    /// if the env var is missing.
    ///
    /// # Errors
    /// Returns [`LlmError::Misconfigured`] if the API key is not set.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
            LlmError::Misconfigured("ANTHROPIC_API_KEY is not set in the environment".into())
        })?;
        Ok(Self {
            api_key,
            base_url: std::env::var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| DEFAULT_BASE_URL.into()),
            http_timeout: Duration::from_secs(60),
        })
    }
}

/// Anthropic Claude provider.
#[derive(Debug)]
pub struct AnthropicProvider {
    config: AnthropicConfig,
    client: reqwest::Client,
}

impl AnthropicProvider {
    /// Construct from config.
    ///
    /// # Errors
    /// Surfaces `reqwest::Client` build failures (rare; typically TLS
    /// initialisation).
    pub fn new(config: AnthropicConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .build()
            .map_err(LlmError::from)?;
        Ok(Self { config, client })
    }

    /// Build using `ANTHROPIC_API_KEY` from the environment.
    ///
    /// # Errors
    /// See [`AnthropicConfig::from_env`].
    pub fn from_env() -> Result<Self> {
        Self::new(AnthropicConfig::from_env()?)
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let wire = WireRequest::from_request(&request);
        let url = format!("{}/v1/messages", self.config.base_url);

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&wire)
            .send()
            .await?;

        let status = response.status();
        let body_bytes = response.bytes().await?;

        if !status.is_success() {
            return Err(map_http_error(status, &body_bytes));
        }

        let wire_resp: WireResponse = serde_json::from_slice(&body_bytes)
            .map_err(|e| LlmError::Decode(format!("anthropic response decode: {e}")))?;
        Ok(wire_resp.into_response(&request.model))
    }
}

fn map_http_error(status: reqwest::StatusCode, body: &[u8]) -> LlmError {
    let message = String::from_utf8_lossy(body).into_owned();
    if status.as_u16() == 429 {
        return LlmError::RateLimited { retry_after: None };
    }
    if status.is_server_error() {
        return LlmError::ServerError {
            status: status.as_u16(),
            message,
        };
    }
    LlmError::ClientError {
        status: status.as_u16(),
        message,
    }
}

// ─── wire types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop_sequences: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireTool>,
}

impl<'a> WireRequest<'a> {
    fn from_request(req: &'a CompletionRequest) -> Self {
        // Anthropic splits system out of the messages array.
        let system = req
            .messages
            .iter()
            .find(|m| m.role == Role::System)
            .map(|m| m.content.clone());
        let messages: Vec<WireMessage> = req
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(WireMessage::from_message)
            .collect();
        let tools: Vec<WireTool> = req
            .tools
            .iter()
            .map(|t| WireTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.parameters.clone(),
            })
            .collect();

        Self {
            model: &req.model,
            messages,
            system,
            // Anthropic requires `max_tokens`. Default to 1024 if the caller
            // didn't pin a value — small enough to keep test costs low.
            max_tokens: Some(req.sampling.max_tokens.unwrap_or(1024)),
            temperature: req.sampling.temperature,
            top_p: req.sampling.top_p,
            stop_sequences: req.sampling.stop.clone(),
            tools,
        }
    }
}

#[derive(Debug, Serialize)]
struct WireMessage {
    role: &'static str,
    content: Vec<WireContentBlock>,
}

impl WireMessage {
    fn from_message(m: &Message) -> Self {
        let role = match m.role {
            Role::User | Role::System => "user",
            Role::Assistant => "assistant",
            Role::Tool => "user",
        };

        let mut blocks: Vec<WireContentBlock> = Vec::new();

        if m.role == Role::Tool {
            blocks.push(WireContentBlock::ToolResult {
                tool_use_id: m.tool_call_id.clone().unwrap_or_default(),
                content: m.content.clone(),
            });
        } else if !m.content.is_empty() {
            blocks.push(WireContentBlock::Text {
                text: m.content.clone(),
            });
        }

        for call in &m.tool_calls {
            blocks.push(WireContentBlock::ToolUse {
                id: call.id.clone(),
                name: call.name.clone(),
                input: call.arguments.clone(),
            });
        }

        Self {
            role,
            content: blocks,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize)]
struct WireTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct WireResponse {
    #[serde(default)]
    id: String,
    model: String,
    content: Vec<WireResponseBlock>,
    stop_reason: Option<String>,
    usage: WireUsage,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireResponseBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Deserialize)]
struct WireUsage {
    input_tokens: u32,
    output_tokens: u32,
}

impl WireResponse {
    fn into_response(self, _requested_model: &str) -> CompletionResponse {
        let mut text = String::new();
        let mut tool_calls = Vec::new();
        for block in self.content {
            match block {
                WireResponseBlock::Text { text: t } => {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&t);
                }
                WireResponseBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments: input,
                    });
                }
            }
        }

        let finish_reason = match self.stop_reason.as_deref() {
            Some("end_turn") | Some("stop_sequence") => FinishReason::Stop,
            Some("max_tokens") => FinishReason::Length,
            Some("tool_use") => FinishReason::ToolCalls,
            _ => FinishReason::Other,
        };

        let usage = Usage {
            prompt_tokens: self.usage.input_tokens,
            completion_tokens: self.usage.output_tokens,
            total_tokens: self
                .usage
                .input_tokens
                .saturating_add(self.usage.output_tokens),
        };

        let mut message = Message::assistant(text);
        message.tool_calls = tool_calls;
        // Tag with provider response id in the content if useful — we drop
        // it for now to keep the message clean.
        let _ = self.id;

        CompletionResponse {
            message,
            finish_reason,
            usage,
            provider: "anthropic".into(),
            model: self.model,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Message, ToolDefinition};

    #[test]
    fn wire_request_splits_system_message() {
        let req = CompletionRequest::chat(
            "claude-3-5-sonnet-20241022",
            vec![Message::system("you are kind"), Message::user("hi")],
        );
        let wire = WireRequest::from_request(&req);
        assert_eq!(wire.system.as_deref(), Some("you are kind"));
        assert_eq!(wire.messages.len(), 1);
        assert_eq!(wire.messages[0].role, "user");
    }

    #[test]
    fn wire_request_includes_tools() {
        let req = CompletionRequest::chat("m", vec![Message::user("x")]).with_tools(vec![
            ToolDefinition::new("click", "click", serde_json::json!({ "type": "object" })),
        ]);
        let wire = WireRequest::from_request(&req);
        assert_eq!(wire.tools.len(), 1);
        assert_eq!(wire.tools[0].name, "click");
    }

    #[test]
    fn response_with_tool_use_maps_to_tool_calls() {
        let json = serde_json::json!({
            "id": "msg_01",
            "model": "claude-3-5-sonnet-20241022",
            "content": [
                { "type": "text", "text": "thinking…" },
                { "type": "tool_use", "id": "tu_01", "name": "click", "input": { "ref": "@e1" } }
            ],
            "stop_reason": "tool_use",
            "usage": { "input_tokens": 12, "output_tokens": 7 }
        });
        let wire: WireResponse = serde_json::from_value(json).unwrap();
        let resp = wire.into_response("claude-3-5-sonnet-20241022");
        assert_eq!(resp.finish_reason, FinishReason::ToolCalls);
        assert_eq!(resp.message.tool_calls.len(), 1);
        assert_eq!(resp.message.tool_calls[0].name, "click");
        assert_eq!(resp.usage.total_tokens, 19);
    }

    #[test]
    fn anthropic_provider_name_is_stable() {
        // Build a provider with a dummy config so we can assert `name()`
        // without touching env vars (Rust 2024 disallows env::remove_var
        // outside an `unsafe` block, which we `forbid`).
        let cfg = AnthropicConfig {
            api_key: "test".into(),
            base_url: "http://invalid.example".into(),
            http_timeout: Duration::from_millis(1),
        };
        let p = AnthropicProvider::new(cfg).expect("client builds");
        assert_eq!(p.name(), "anthropic");
    }
}
