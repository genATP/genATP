//! Ollama adapter — local LLM server, no auth.
//!
//! Useful in CI because there is no API key to manage and the server runs
//! on `127.0.0.1:11434` by default. Ollama's tool-calling support landed
//! in version 0.3 (July 2024); older versions silently drop tool definitions
//! and only return text replies — we do not try to detect this; the
//! integration test pin a known model.
//!
//! Reference: <https://github.com/ollama/ollama/blob/main/docs/api.md>

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{LlmError, Result};
use crate::provider::LlmProvider;
use crate::types::{
    CompletionRequest, CompletionResponse, FinishReason, Message, Role, ToolCall, Usage,
};

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:11434";

/// Adapter configuration.
#[derive(Debug, Clone)]
pub struct OllamaConfig {
    pub base_url: String,
    pub http_timeout: Duration,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into()),
            http_timeout: Duration::from_secs(120),
        }
    }
}

/// Ollama provider.
#[derive(Debug)]
pub struct OllamaProvider {
    config: OllamaConfig,
    client: reqwest::Client,
}

impl OllamaProvider {
    /// Construct from config.
    ///
    /// # Errors
    /// Surfaces `reqwest::Client` build failures.
    pub fn new(config: OllamaConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .build()
            .map_err(LlmError::from)?;
        Ok(Self { config, client })
    }

    /// Default config (env-driven base URL, 120 s timeout).
    ///
    /// # Errors
    /// Surfaces `reqwest::Client` build failures.
    pub fn from_env() -> Result<Self> {
        Self::new(OllamaConfig::default())
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &'static str {
        "ollama"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let wire = WireRequest::from_request(&request);
        let url = format!("{}/api/chat", self.config.base_url);
        let response = self.client.post(&url).json(&wire).send().await?;
        let status = response.status();
        let body_bytes = response.bytes().await?;
        if !status.is_success() {
            return Err(LlmError::ServerError {
                status: status.as_u16(),
                message: String::from_utf8_lossy(&body_bytes).into_owned(),
            });
        }
        let wire_resp: WireResponse = serde_json::from_slice(&body_bytes)
            .map_err(|e| LlmError::Decode(format!("ollama response decode: {e}")))?;
        Ok(wire_resp.into_response(&request.model))
    }
}

// ─── wire types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage>,
    /// `false` — orchestrator runs one request at a time.
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<WireOptions>,
}

impl<'a> WireRequest<'a> {
    fn from_request(req: &'a CompletionRequest) -> Self {
        Self {
            model: &req.model,
            messages: req.messages.iter().map(WireMessage::from_message).collect(),
            stream: false,
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
            options: Self::map_options(&req.sampling),
        }
    }

    fn map_options(sampling: &crate::types::SamplingParams) -> Option<WireOptions> {
        if sampling.temperature.is_none()
            && sampling.top_p.is_none()
            && sampling.max_tokens.is_none()
            && sampling.stop.is_empty()
        {
            return None;
        }
        Some(WireOptions {
            temperature: sampling.temperature,
            top_p: sampling.top_p,
            num_predict: sampling.max_tokens.map(i32::try_from).and_then(|r| r.ok()),
            stop: if sampling.stop.is_empty() {
                None
            } else {
                Some(sampling.stop.clone())
            },
        })
    }
}

#[derive(Debug, Serialize)]
struct WireMessage {
    role: &'static str,
    content: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<WireMsgToolCall>,
}

impl WireMessage {
    fn from_message(m: &Message) -> Self {
        let role = match m.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };
        Self {
            role,
            content: m.content.clone(),
            tool_calls: m
                .tool_calls
                .iter()
                .map(|c| WireMsgToolCall {
                    function: WireMsgToolCallFn {
                        name: c.name.clone(),
                        arguments: c.arguments.clone(),
                    },
                })
                .collect(),
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
struct WireMsgToolCall {
    function: WireMsgToolCallFn,
}

#[derive(Debug, Serialize)]
struct WireMsgToolCallFn {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct WireOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct WireResponse {
    #[serde(default)]
    model: String,
    message: WireRespMessage,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct WireRespMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Vec<WireRespToolCall>,
}

#[derive(Debug, Deserialize)]
struct WireRespToolCall {
    #[serde(default)]
    id: Option<String>,
    function: WireRespToolCallFn,
}

#[derive(Debug, Deserialize)]
struct WireRespToolCallFn {
    name: String,
    arguments: serde_json::Value,
}

impl WireResponse {
    fn into_response(self, requested_model: &str) -> CompletionResponse {
        let mut message = Message::assistant(self.message.content);
        message.tool_calls = self
            .message
            .tool_calls
            .into_iter()
            .enumerate()
            .map(|(idx, c)| ToolCall {
                id: c.id.unwrap_or_else(|| format!("call_{idx}")),
                name: c.function.name,
                arguments: c.function.arguments,
            })
            .collect();
        let finish_reason = if !message.tool_calls.is_empty() {
            FinishReason::ToolCalls
        } else if self.done {
            FinishReason::Stop
        } else {
            FinishReason::Other
        };
        let prompt_tokens = self.prompt_eval_count.unwrap_or(0);
        let completion_tokens = self.eval_count.unwrap_or(0);
        let usage = Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens.saturating_add(completion_tokens),
        };
        CompletionResponse {
            message,
            finish_reason,
            usage,
            provider: "ollama".into(),
            model: if self.model.is_empty() {
                requested_model.to_string()
            } else {
                self.model
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_response_with_tool_calls_maps_to_finish_tool_calls() {
        let json = serde_json::json!({
            "model": "llama3.1",
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "function": {
                        "name": "click",
                        "arguments": { "ref": "@e1" }
                    }
                }]
            },
            "done": true,
            "prompt_eval_count": 12,
            "eval_count": 7
        });
        let wire: WireResponse = serde_json::from_value(json).unwrap();
        let resp = wire.into_response("llama3.1");
        assert_eq!(resp.finish_reason, FinishReason::ToolCalls);
        assert_eq!(resp.message.tool_calls.len(), 1);
        assert_eq!(resp.message.tool_calls[0].name, "click");
        assert_eq!(resp.usage.total_tokens, 19);
    }

    #[test]
    fn empty_done_maps_to_stop() {
        let json = serde_json::json!({
            "model": "llama3.1",
            "message": { "role": "assistant", "content": "hello" },
            "done": true
        });
        let wire: WireResponse = serde_json::from_value(json).unwrap();
        let resp = wire.into_response("llama3.1");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert_eq!(resp.message.content, "hello");
    }
}
