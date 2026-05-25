//! Vision adapter — scaffold only.
//!
//! Phase 2 ships the trait and a [`NullVisionProvider`] so the rest of the
//! engine can compile and test against the same interface that Phase 12 will
//! later wire up to Anthropic Claude, `OpenAI` GPT-4o, and Google Gemini.
//!
//! The trait deliberately stays narrow:
//!
//! - **Input**: a screenshot (PNG bytes) plus a prompt describing the task.
//! - **Output**: a [`VisionResponse`] containing model output text plus
//!   structured action hints (clickable regions, OCR text spans). Phase 12
//!   refines the structure based on the policy engine's demands.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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

/// What the trait returns. Phase 12 will likely add `usage` (tokens) and
/// `model` fields once the provider clients land.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct VisionResponse {
    /// Free-form text reply (e.g. "I see a login form with username and
    /// password fields"). Empty if the provider only returned structured
    /// data.
    pub text: String,
    /// Structured action hints, in the order the model thinks the user
    /// should consider them.
    pub actions: Vec<VisionAction>,
}

/// Errors a vision provider can surface. Kept minimal in Phase 2.
#[derive(Debug, thiserror::Error)]
pub enum VisionError {
    /// Adapter is not yet implemented (Phase 12 fills these in).
    #[error("vision adapter not yet implemented: {0}")]
    NotImplemented(&'static str),

    /// Underlying provider transport / auth / quota failure.
    #[error("vision provider error: {0}")]
    Provider(String),
}

/// Minimal provider trait. `async_trait` so adapters can dispatch async
/// calls; downstream impls in Phase 12 will be sync-fn-where-possible to keep
/// the hot path lean.
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
    ) -> Result<VisionResponse, VisionError>;
}

/// Inert provider that never calls a network. Returns
/// [`VisionError::NotImplemented`]. Used as the default in the orchestrator
/// until Phase 12 ships real adapters.
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
    ) -> Result<VisionResponse, VisionError> {
        Err(VisionError::NotImplemented(
            "null provider — wire in Phase 12",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn null_provider_returns_not_implemented() {
        let p = NullVisionProvider;
        assert_eq!(p.name(), "null");
        let err = p.complete(&[], "look at this").await.expect_err("must err");
        assert!(matches!(err, VisionError::NotImplemented(_)));
    }
}
