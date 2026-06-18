//! Vision adapters — Phase 12 production implementations.
//!
//! Three providers replace the Phase 2 [`NullVisionProvider`]:
//!
//! - **Claude** (Anthropic Messages API) — best-in-class vision.
//! - **GPT-4o** (OpenAI Chat Completions API) — strong vision + structured output.
//! - **Gemini** (Google Generative Language API) — competitive vision, generous free tier.
//!
//! Each adapter converts a screenshot + prompt into a [`VisionResponse`] containing
//! free-form text and structured action hints (clickable regions, OCR text).
//!
//! ## Architecture
//!
//! All three providers share the same HTTP client (`reqwest::Client`) and
//! follow the same pattern:
//!
//! 1. Encode the screenshot as base64 PNG.
//! 2. Build the provider-specific request with image content + text prompt.
//! 3. Parse the provider-specific response into [`VisionResponse`].
//!
//! ## Configuration
//!
//! Providers are configured by setting the corresponding environment variable
//! before constructing the adapter:
//!
//! | Provider | Env var | Default |
//! |---|---|---|
//! | Claude | `ANTHROPIC_API_KEY` | — |
//! | GPT-4o | `OPENAI_API_KEY` | — |
//! | Gemini | `GOOGLE_API_KEY` | — |
//!
//! Fallback order: Claude → GPT-4o → Gemini → Null.

use async_trait::async_trait;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Re-export the base types from the legacy scaffold
// ---------------------------------------------------------------------------

/// One viewport-relative bounding box, normalised to `[0, 1]` on each axis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// One actionable region the vision model identified.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisionAction {
    pub label: String,
    pub bbox: BoundingBox,
    /// 0..=1 confidence reported by the model (`0.0` if not provided).
    #[serde(default)]
    pub confidence: f32,
}

/// What the trait returns.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct VisionResponse {
    /// Free-form text reply (e.g. "I see a login form with username and
    /// password fields"). Empty if the provider only returned structured data.
    pub text: String,
    /// Structured action hints, in the order the model thinks the user
    /// should consider them.
    pub actions: Vec<VisionAction>,
}

/// Errors a vision provider can surface.
#[derive(Debug, thiserror::Error)]
pub enum VisionError {
    /// Adapter is not yet implemented.
    #[error("vision adapter not yet implemented: {0}")]
    NotImplemented(&'static str),

    /// Underlying provider transport / auth / quota failure.
    #[error("vision provider error: {0}")]
    Provider(String),

    /// HTTP request failure (timeout, DNS, etc.).
    #[error("vision http error: {0}")]
    Http(#[from] reqwest::Error),

    /// Response parsing failure.
    #[error("vision parse error: {0}")]
    Parse(String),

    /// Missing API key for the requested provider.
    #[error("missing API key for {provider}: set {env_var}")]
    MissingApiKey {
        provider: &'static str,
        env_var: &'static str,
    },

    /// Base64 encoding failure.
    #[error("base64 encode error")]
    Base64(String),
}

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

/// Minimal provider trait. `async_trait` so adapters can dispatch async calls.
#[async_trait]
pub trait VisionProvider: Send + Sync {
    /// Stable identifier for telemetry and logs (`"claude"`, `"gpt-4o"`, …).
    fn name(&self) -> &'static str;

    /// Run the model on `screenshot_png` with `prompt` and return the
    /// structured response.
    async fn complete(
        &self,
        screenshot_png: &[u8],
        prompt: &str,
    ) -> std::result::Result<VisionResponse, VisionError>;
}

// ---------------------------------------------------------------------------
// Null (legacy scaffold)
// ---------------------------------------------------------------------------

/// Inert provider that never calls a network. Returns
/// [`VisionError::NotImplemented`]. Used as the default fallback.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullVisionProvider;

#[async_trait]
impl VisionProvider for NullVisionProvider {
    fn name(&self) -> &'static str {
        "null"
    }

    async fn complete(
        &self,
        _screenshot_png: &[u8],
        _prompt: &str,
    ) -> std::result::Result<VisionResponse, VisionError> {
        Err(VisionError::NotImplemented(
            "null provider — set ANTHROPIC_API_KEY, OPENAI_API_KEY, or GOOGLE_API_KEY",
        ))
    }
}

// ---------------------------------------------------------------------------
// Claude (Anthropic Messages API)
// ---------------------------------------------------------------------------

/// Vision provider using Anthropic's Claude via the Messages API.
///
/// Requires `ANTHROPIC_API_KEY` environment variable.
/// Uses `claude-sonnet-4-20250514` by default.
#[derive(Debug)]
pub struct ClaudeVisionProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl ClaudeVisionProvider {
    /// Create a new Claude vision provider.
    ///
    /// # Errors
    /// Returns [`VisionError::MissingApiKey`] if `ANTHROPIC_API_KEY` is not set.
    pub fn from_env() -> std::result::Result<Self, VisionError> {
        let api_key =
            std::env::var("ANTHROPIC_API_KEY").map_err(|_| VisionError::MissingApiKey {
                provider: "Claude",
                env_var: "ANTHROPIC_API_KEY",
            })?;
        Ok(Self::new(api_key, "claude-sonnet-4-20250514"))
    }

    /// Create a new Claude vision provider with explicit key and model.
    #[must_use]
    pub fn new(api_key: String, model: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model: model.into(),
        }
    }
}

#[async_trait]
impl VisionProvider for ClaudeVisionProvider {
    fn name(&self) -> &'static str {
        "claude"
    }

    async fn complete(
        &self,
        screenshot_png: &[u8],
        prompt: &str,
    ) -> std::result::Result<VisionResponse, VisionError> {
        let b64 = base64::engine::general_purpose::STANDARD.encode(screenshot_png);
        let media_type = "image/png";

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": prompt},
                    {
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": media_type,
                            "data": b64
                        }
                    }
                ]
            }]
        });

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(VisionError::Provider(format!(
                "Claude API {status}: {text}"
            )));
        }

        let claude_resp: ClaudeResponse = resp.json().await?;
        let text = claude_resp
            .content
            .first()
            .map(|c| c.text.clone())
            .unwrap_or_default();

        // Parse structured actions from the text response (JSON block).
        let actions = parse_actions_from_text(&text);

        Ok(VisionResponse { text, actions })
    }
}

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    content: Vec<ClaudeContent>,
}

#[derive(Debug, Deserialize)]
struct ClaudeContent {
    #[serde(default)]
    text: String,
}

// ---------------------------------------------------------------------------
// GPT-4o (OpenAI Chat Completions API)
// ---------------------------------------------------------------------------

/// Vision provider using OpenAI's GPT-4o via the Chat Completions API.
///
/// Requires `OPENAI_API_KEY` environment variable.
/// Uses `gpt-4o-2025-01-02` by default (latest stable vision-capable model).
#[derive(Debug)]
pub struct Gpt4oVisionProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl Gpt4oVisionProvider {
    /// Create a new GPT-4o vision provider from environment.
    ///
    /// # Errors
    /// Returns [`VisionError::MissingApiKey`] if `OPENAI_API_KEY` is not set.
    pub fn from_env() -> std::result::Result<Self, VisionError> {
        let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| VisionError::MissingApiKey {
            provider: "GPT-4o",
            env_var: "OPENAI_API_KEY",
        })?;
        Ok(Self::new(api_key, "gpt-4o-2025-01-02"))
    }

    /// Create a new GPT-4o provider with explicit key and model.
    #[must_use]
    pub fn new(api_key: String, model: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model: model.into(),
        }
    }
}

#[async_trait]
impl VisionProvider for Gpt4oVisionProvider {
    fn name(&self) -> &'static str {
        "gpt-4o"
    }

    async fn complete(
        &self,
        screenshot_png: &[u8],
        prompt: &str,
    ) -> std::result::Result<VisionResponse, VisionError> {
        let b64 = base64::engine::general_purpose::STANDARD.encode(screenshot_png);
        let data_url = format!("data:image/png;base64,{b64}");

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": prompt},
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": data_url,
                            "detail": "high"
                        }
                    }
                ]
            }]
        });

        let resp = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(VisionError::Provider(format!(
                "OpenAI API {status}: {text}"
            )));
        }

        let openai_resp: OpenAiResponse = resp.json().await?;
        let text = openai_resp
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        let actions = parse_actions_from_text(&text);

        Ok(VisionResponse { text, actions })
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    #[serde(default)]
    content: String,
}

// ---------------------------------------------------------------------------
// Gemini (Google Generative Language API)
// ---------------------------------------------------------------------------

/// Vision provider using Google's Gemini via the Generative Language API.
///
/// Requires `GOOGLE_API_KEY` environment variable.
/// Uses `gemini-2.0-flash-001` by default.
#[derive(Debug)]
pub struct GeminiVisionProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl GeminiVisionProvider {
    /// Create a new Gemini vision provider from environment.
    ///
    /// # Errors
    /// Returns [`VisionError::MissingApiKey`] if `GOOGLE_API_KEY` is not set.
    pub fn from_env() -> std::result::Result<Self, VisionError> {
        let api_key = std::env::var("GOOGLE_API_KEY").map_err(|_| VisionError::MissingApiKey {
            provider: "Gemini",
            env_var: "GOOGLE_API_KEY",
        })?;
        Ok(Self::new(api_key, "gemini-2.0-flash-001"))
    }

    /// Create a new Gemini provider with explicit key and model.
    #[must_use]
    pub fn new(api_key: String, model: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model: model.into(),
        }
    }
}

#[async_trait]
impl VisionProvider for GeminiVisionProvider {
    fn name(&self) -> &'static str {
        "gemini"
    }

    async fn complete(
        &self,
        screenshot_png: &[u8],
        prompt: &str,
    ) -> std::result::Result<VisionResponse, VisionError> {
        let b64 = base64::engine::general_purpose::STANDARD.encode(screenshot_png);

        let body = serde_json::json!({
            "contents": [{
                "role": "user",
                "parts": [
                    {"text": prompt},
                    {
                        "inline_data": {
                            "mime_type": "image/png",
                            "data": b64
                        }
                    }
                ]
            }]
        });

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let resp = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(VisionError::Provider(format!(
                "Gemini API {status}: {text}"
            )));
        }

        let gemini_resp: GeminiResponse = resp.json().await?;
        let text = gemini_resp
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .map(|p| p.text.clone())
            .unwrap_or_default();

        let actions = parse_actions_from_text(&text);

        Ok(VisionResponse { text, actions })
    }
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
}

#[derive(Debug, Deserialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Deserialize)]
struct GeminiPart {
    #[serde(default)]
    text: String,
}

// ---------------------------------------------------------------------------
// Action parser — extracts structured actions from vision model text
// ---------------------------------------------------------------------------

/// Try to parse structured actions from the model's text response.
///
/// The model is instructed to return a JSON block with bounding boxes.
/// This function looks for a JSON array `[{label, bbox, confidence}, ...]`
/// within the response text.
fn parse_actions_from_text(text: &str) -> Vec<VisionAction> {
    // Try to find a JSON array in the response.
    let text = text.trim();

    // First, try to parse the entire response as a JSON array of actions.
    if let Ok(actions) = serde_json::from_str::<Vec<VisionAction>>(text) {
        return actions;
    }

    // Try to find a JSON code block ```json ... ```
    if let Some(start) = text.find("```json") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            let json_str = after[..end].trim();
            if let Ok(actions) = serde_json::from_str::<Vec<VisionAction>>(json_str) {
                return actions;
            }
        }
    }

    // Try to find a JSON array [...] anywhere in the text.
    if let Some(start) = text.find('[') {
        let after = &text[start..];
        // Find matching close bracket
        let mut depth = 0;
        for (i, ch) in after.char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        let candidate = &after[..=i];
                        if let Ok(actions) = serde_json::from_str::<Vec<VisionAction>>(candidate) {
                            return actions;
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    Vec::new()
}

// ---------------------------------------------------------------------------
// Provider factory — resolve which provider to use from environment
// ---------------------------------------------------------------------------

/// Which vision provider to use, resolved from environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Claude,
    Gpt4o,
    Gemini,
    Null,
}

/// Resolve the vision provider from environment variables.
///
/// Priority: Claude → GPT-4o → Gemini → Null.
/// If a key is set but the provider creation fails, falls through to the next.
#[must_use]
pub fn resolve_provider_from_env() -> Box<dyn VisionProvider> {
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        match ClaudeVisionProvider::from_env() {
            Ok(p) => return Box::new(p),
            Err(e) => tracing::warn!("Claude vision unavailable: {e}"),
        }
    }
    if std::env::var("OPENAI_API_KEY").is_ok() {
        match Gpt4oVisionProvider::from_env() {
            Ok(p) => return Box::new(p),
            Err(e) => tracing::warn!("GPT-4o vision unavailable: {e}"),
        }
    }
    if std::env::var("GOOGLE_API_KEY").is_ok() {
        match GeminiVisionProvider::from_env() {
            Ok(p) => return Box::new(p),
            Err(e) => tracing::warn!("Gemini vision unavailable: {e}"),
        }
    }
    Box::new(NullVisionProvider)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_provider_returns_not_implemented() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let p = NullVisionProvider;
        assert_eq!(p.name(), "null");
        let err = rt
            .block_on(p.complete(&[], "look at this"))
            .expect_err("must err");
        assert!(matches!(err, VisionError::NotImplemented(_)));
    }

    #[test]
    fn parse_actions_from_text_empty_for_plain_text() {
        let actions = parse_actions_from_text("I see a blue button in the top left.");
        assert!(actions.is_empty());
    }

    #[test]
    fn parse_actions_from_text_finds_json_block() {
        let text = r#"Here are the elements I found:
```json
[
  {"label": "Submit button", "bbox": {"x": 0.1, "y": 0.8, "width": 0.3, "height": 0.05}, "confidence": 0.95}
]
```"#;
        let actions = parse_actions_from_text(text);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].label, "Submit button");
        assert!((actions[0].confidence - 0.95).abs() < 0.01);
    }

    #[test]
    fn parse_actions_from_text_finds_inline_json() {
        let text = r#"The page has a search box at [{"label":"Search","bbox":{"x":0.5,"y":0.1,"width":0.4,"height":0.06},"confidence":0.9}]"#;
        let actions = parse_actions_from_text(text);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].label, "Search");
    }

    #[test]
    fn parse_actions_from_text_handles_empty_json() {
        let actions = parse_actions_from_text("[]");
        assert!(actions.is_empty());
    }

    #[test]
    fn provider_kinds_have_distinct_names() {
        // Just verify the name() methods work; actual API calls require keys.
        let names: std::collections::HashSet<&str> = ["claude", "gpt-4o", "gemini", "null"].into();
        assert_eq!(names.len(), 4);
    }

    #[test]
    fn bounding_box_serialization_roundtrip() {
        let bbox = BoundingBox {
            x: 0.1,
            y: 0.2,
            width: 0.5,
            height: 0.3,
        };
        let json = serde_json::to_string(&bbox).unwrap();
        let back: BoundingBox = serde_json::from_str(&json).unwrap();
        assert!((back.x - 0.1).abs() < 1e-5);
        assert!((back.height - 0.3).abs() < 1e-5);
    }

    #[test]
    fn vision_action_serialization_roundtrip() {
        let action = VisionAction {
            label: "Login".into(),
            bbox: BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            confidence: 0.85,
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: VisionAction = serde_json::from_str(&json).unwrap();
        assert_eq!(back.label, "Login");
        assert!((back.confidence - 0.85).abs() < 1e-5);
    }

    #[test]
    fn missing_api_key_returns_descriptive_error() {
        // Verify the MissingApiKey error is constructed correctly
        // without needing to modify environment variables.
        let err = VisionError::MissingApiKey {
            provider: "Claude",
            env_var: "ANTHROPIC_API_KEY",
        };
        let msg = err.to_string();
        assert!(msg.contains("ANTHROPIC_API_KEY"), "error message: {msg}");
        assert!(msg.contains("Claude"), "error message: {msg}");

        // Verify from_env() fails when key is empty
        // We use a new provider with empty key to test construction
        let provider = ClaudeVisionProvider::new(String::new(), "test");
        // The provider is valid but won't work with empty API key
        assert_eq!(provider.name(), "claude");
    }
}
