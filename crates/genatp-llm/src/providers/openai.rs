//! OpenAI Chat Completions adapter (GPT-4o and friends).
//!
//! Endpoint contract:
//! - URL: `<base_url>/v1/chat/completions`
//! - Auth: `Authorization: Bearer <api_key>` header
//!
//! Reference: <https://platform.openai.com/docs/api-reference/chat>

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{LlmError, Result};
use crate::provider::LlmProvider;
use crate::types::{
    CompletionRequest, CompletionResponse, FinishReason, Message, Role, ToolCall, Usage,
};

const DEFAULT_BASE_URL: &str = "https://api.openai.com";

/// Adapter configuration.
#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    pub api_key: String,
    pub base_url: String,
    pub http_timeout: Duration,
    /// Optional org id (`OpenAI-Organization` header).
    pub organization: Option<String>,
}

impl OpenAiConfig {
    /// Build from environment.
    ///
    /// # Errors
    /// Returns [`LlmError::Misconfigured`] if `OPENAI_API_KEY` is unset.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| LlmError::Misconfigured("OPENAI_API_KEY is not set".into()))?;
        Ok(Self {
            api_key,
            base_url: std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into()),
            http_timeout: Duration::from_secs(60),
            organization: std::env::var("OPENAI_ORG").ok(),
        })
    }
}

/// OpenAI provider.
#[derive(Debug)]
pub struct OpenAiProvider {
    config: OpenAiConfig,
    client: reqwest::Client,
}

impl OpenAiProvider {
    /// Construct from config.
    ///
    /// # Errors
    /// Surfaces `reqwest::Client` build failures.
    pub fn new(config: OpenAiConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .build()
            .map_err(LlmError::from)?;
        Ok(Self { config, client })
    }

    /// Construct from environment.
    ///
    /// # Errors
    /// See [`OpenAiConfig::from_env`].
    pub fn from_env() -> Result<Self> {
        Self::new(OpenAiConfig::from_env()?)
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let wire = WireRequest::from_request(&request);
        let url = format!("{}/v1/chat/completions", self.config.base_url);

        let mut builder = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&wire);
        if let Some(org) = &self.config.organization {
            builder = builder.header("OpenAI-Organization", org);
        }

        let response = builder.send().await?;
        let status = response.status();
        let body_bytes = response.bytes().await?;
        if !status.is_success() {
            return Err(map_http_error(status, &body_bytes));
        }
        let wire_resp: WireResponse = serde_json::from_slice(&body_bytes)
            .map_err(|e| LlmError::Decode(format!("openai response decode: {e}")))?;
        Ok(wire_resp.into_response())
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
}

impl<'a> WireRequest<'a> {
    fn from_request(req: &'a CompletionRequest) -> Self {
        Self {
            model: &req.model,
            messages: req.messages.iter().map(WireMessage::from_message).collect(),
            tools: req
                .tools
                .iter()
                .map(|t| WireTool {
                    r#type: "function",
                    function: WireToolFn {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.parameters.clone(),
                    },
                })
                .collect(),
            temperature: req.sampling.temperature,
            top_p: req.sampling.top_p,
            max_tokens: req.sampling.max_tokens,
            stop: req.sampling.stop.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct WireMessage {
    role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<WireToolCall>,
}

impl WireMessage {
    fn from_message(m: &Message) -> Self {
        let role = match m.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };
        let tool_calls = m
            .tool_calls
            .iter()
            .map(|c| WireToolCall {
                id: c.id.clone(),
                r#type: "function",
                function: WireToolCallFn {
                    name: c.name.clone(),
                    arguments: c.arguments.to_string(),
                },
            })
            .collect();
        Self {
            role,
            content: if m.content.is_empty() {
                None
            } else {
                Some(m.content.clone())
            },
            tool_call_id: m.tool_call_id.clone(),
            tool_calls,
        }
    }
}

#[derive(Debug, Serialize)]
struct WireTool {
    r#type: &'static str,
    function: WireToolFn,
}

#[derive(Debug, Serialize)]
struct WireToolFn {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct WireToolCall {
    id: String,
    r#type: &'static str,
    function: WireToolCallFn,
}

#[derive(Debug, Serialize)]
struct WireToolCallFn {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct WireResponse {
    #[allow(dead_code)]
    id: String,
    model: String,
    choices: Vec<WireChoice>,
    #[serde(default)]
    usage: Option<WireUsage>,
}

#[derive(Debug, Deserialize)]
struct WireChoice {
    message: WireResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<WireResponseToolCall>,
}

#[derive(Debug, Deserialize)]
struct WireResponseToolCall {
    id: String,
    function: WireResponseFn,
}

#[derive(Debug, Deserialize)]
struct WireResponseFn {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct WireUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    #[serde(default)]
    total_tokens: u32,
}

impl WireResponse {
    fn into_response(self) -> CompletionResponse {
        let choice = self.choices.into_iter().next().unwrap_or(WireChoice {
            message: WireResponseMessage {
                content: None,
                tool_calls: Vec::new(),
            },
            finish_reason: None,
        });

        let mut message = Message::assistant(choice.message.content.unwrap_or_default());
        message.tool_calls = choice
            .message
            .tool_calls
            .into_iter()
            .map(|c| ToolCall {
                id: c.id,
                name: c.function.name,
                arguments: serde_json::from_str(&c.function.arguments)
                    .unwrap_or_else(|_| serde_json::json!({})),
            })
            .collect();

        let finish_reason = match choice.finish_reason.as_deref() {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::Length,
            Some("tool_calls") | Some("function_call") => FinishReason::ToolCalls,
            Some("content_filter") => FinishReason::ContentFilter,
            _ => FinishReason::Other,
        };

        let usage = self
            .usage
            .map(|u| Usage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: if u.total_tokens > 0 {
                    u.total_tokens
                } else {
                    u.prompt_tokens.saturating_add(u.completion_tokens)
                },
            })
            .unwrap_or_default();

        CompletionResponse {
            message,
            finish_reason,
            usage,
            provider: "openai".into(),
            model: self.model,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Message;

    #[test]
    fn wire_request_includes_role_strings() {
        let req =
            CompletionRequest::chat("gpt-4o", vec![Message::system("sys"), Message::user("hi")]);
        let wire = WireRequest::from_request(&req);
        assert_eq!(wire.messages[0].role, "system");
        assert_eq!(wire.messages[1].role, "user");
    }

    #[test]
    fn response_with_tool_calls_maps_correctly() {
        let json = serde_json::json!({
            "id": "chatcmpl-123",
            "model": "gpt-4o-2024-08-06",
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "click",
                            "arguments": "{\"ref\":\"@e1\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15 }
        });
        let wire: WireResponse = serde_json::from_value(json).unwrap();
        let resp = wire.into_response();
        assert_eq!(resp.finish_reason, FinishReason::ToolCalls);
        assert_eq!(resp.message.tool_calls.len(), 1);
        assert_eq!(resp.message.tool_calls[0].name, "click");
        assert_eq!(
            resp.message.tool_calls[0].arguments,
            serde_json::json!({"ref":"@e1"})
        );
        assert_eq!(resp.usage.total_tokens, 15);
    }

    #[test]
    fn openai_provider_name_is_stable() {
        let cfg = OpenAiConfig {
            api_key: "test".into(),
            base_url: "http://invalid.example".into(),
            http_timeout: Duration::from_millis(1),
            organization: None,
        };
        let p = OpenAiProvider::new(cfg).expect("client builds");
        assert_eq!(p.name(), "openai");
    }
}
