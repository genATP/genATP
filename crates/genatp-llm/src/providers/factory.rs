//! Provider auto-selection from environment variables.
//!
//! genATP supports four real LLM backends and one OpenAI-compatible
//! integration (NVIDIA NIM). End users should not have to know which
//! adapter to construct by hand — this module reads a small set of
//! environment variables and returns a ready-to-use
//! `Arc<dyn LlmProvider>` plus the model id the orchestrator should pass.
//!
//! ## Selection precedence
//!
//! 1. If `GENATP_LLM_PROVIDER` is set, that wins (one of:
//!    `anthropic`, `openai`, `nim`, `ollama`).
//! 2. Otherwise we auto-detect in this order:
//!    - `ANTHROPIC_API_KEY` set → Anthropic
//!    - `NVIDIA_API_KEY` set (or `OPENAI_BASE_URL` points at
//!      `integrate.api.nvidia.com`) → NVIDIA NIM (OpenAI-wire)
//!    - `OPENAI_API_KEY` set → OpenAI
//!    - otherwise → Ollama at `http://127.0.0.1:11434`
//!
//! ## Model defaults
//!
//! If `GENATP_LLM_MODEL` is set, that wins. Otherwise we pick a sensible
//! default per provider — see [`ResolvedProvider::model`].
//!
//! ## Local-first vs. API-first
//!
//! Both modes are first-class. The auto-detection above means the same
//! code works for a developer running Ollama locally **and** for someone
//! using a hosted API — no recompilation, no feature flag, just env vars.
//! See `docs/CONFIGURATION.md` for the full guide.

#[cfg(feature = "http")]
use std::sync::Arc;

#[cfg(feature = "http")]
use crate::error::{LlmError, Result};
#[cfg(feature = "http")]
use crate::provider::LlmProvider;
#[cfg(feature = "http")]
use crate::providers::{
    anthropic::{AnthropicConfig, AnthropicProvider},
    ollama::{OllamaConfig, OllamaProvider},
    openai::{OpenAiConfig, OpenAiProvider},
};

/// One of the supported backend kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// `anthropic` — Claude Messages API.
    Anthropic,
    /// `openai` — OpenAI Chat Completions API.
    OpenAi,
    /// `nim` — NVIDIA NIM, OpenAI-wire compatible at
    /// `https://integrate.api.nvidia.com`.
    Nim,
    /// `ollama` — local Ollama daemon, default
    /// `http://127.0.0.1:11434`.
    Ollama,
}

impl ProviderKind {
    /// Parse a case-insensitive kind from a string. Accepts the short
    /// aliases `claude`/`gpt`/`nvidia` for human-friendliness.
    ///
    /// # Errors
    /// Returns [`LlmError::Misconfigured`] if the value is unknown.
    #[cfg(feature = "http")]
    pub fn parse(value: &str) -> Result<Self> {
        Ok(match value.trim().to_ascii_lowercase().as_str() {
            "anthropic" | "claude" => Self::Anthropic,
            "openai" | "gpt" => Self::OpenAi,
            "nim" | "nvidia" => Self::Nim,
            "ollama" | "local" => Self::Ollama,
            other => {
                return Err(LlmError::Misconfigured(format!(
                    "unknown GENATP_LLM_PROVIDER `{other}` — expected one of \
                     anthropic, openai, nim, ollama"
                )));
            }
        })
    }

    /// Stable lowercase identifier (matches [`LlmProvider::name`] for the
    /// underlying adapter; NIM reports as `openai`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::Nim => "nim",
            Self::Ollama => "ollama",
        }
    }

    /// Default model id used when `GENATP_LLM_MODEL` is unset.
    ///
    /// These are conservative picks — small, cheap, widely-available
    /// models that support tool calling. Override via env var for
    /// anything serious.
    #[must_use]
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Anthropic => "claude-3-5-sonnet-latest",
            Self::OpenAi => "gpt-4o-mini",
            // Picked because it's listed for "agentic coding and browser use"
            // in the NVIDIA NIM catalogue and supports tool calling.
            Self::Nim => "qwen/qwen3-coder-480b-a35b-instruct",
            // 8B llama is the smallest model with reliable tool calling.
            Self::Ollama => "llama3.1:8b",
        }
    }
}

/// What [`resolve_from_env`] hands back: a constructed provider plus the
/// model id the orchestrator should send in each `CompletionRequest`.
#[cfg(feature = "http")]
pub struct ResolvedProvider {
    /// The backend that was selected.
    pub kind: ProviderKind,
    /// Ready-to-use provider, shareable across tasks.
    pub provider: Arc<dyn LlmProvider>,
    /// Default model id (overridable by the caller per-request).
    pub model: String,
}

#[cfg(feature = "http")]
impl std::fmt::Debug for ResolvedProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedProvider")
            .field("kind", &self.kind)
            .field("model", &self.model)
            .field("provider", &self.provider.name())
            .finish()
    }
}

/// Inspect the environment and return a ready-to-use provider.
///
/// See the [module docs](self) for selection precedence.
///
/// # Errors
/// - [`LlmError::Misconfigured`] if `GENATP_LLM_PROVIDER` is set to an
///   unknown value, or if the explicitly-selected backend's required
///   credentials are missing.
/// - Surfaces `reqwest::Client` build errors.
#[cfg(feature = "http")]
pub fn resolve_from_env() -> Result<ResolvedProvider> {
    let explicit = std::env::var("GENATP_LLM_PROVIDER").ok();
    let kind = match explicit.as_deref() {
        Some(v) if !v.is_empty() => ProviderKind::parse(v)?,
        _ => detect()?,
    };
    build(kind)
}

/// Build a provider for the given kind, reading credentials and base
/// URLs from the environment.
///
/// # Errors
/// As [`resolve_from_env`].
#[cfg(feature = "http")]
pub fn build(kind: ProviderKind) -> Result<ResolvedProvider> {
    let model = std::env::var("GENATP_LLM_MODEL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| kind.default_model().to_owned());

    let provider: Arc<dyn LlmProvider> = match kind {
        ProviderKind::Anthropic => Arc::new(AnthropicProvider::new(AnthropicConfig::from_env()?)?),
        ProviderKind::OpenAi => Arc::new(OpenAiProvider::new(OpenAiConfig::from_env()?)?),
        ProviderKind::Nim => Arc::new(OpenAiProvider::new(nim_config_from_env()?)?),
        ProviderKind::Ollama => Arc::new(OllamaProvider::new(OllamaConfig::default())?),
    };

    Ok(ResolvedProvider {
        kind,
        provider,
        model,
    })
}

/// Auto-detect the kind without an explicit override.
#[cfg(feature = "http")]
fn detect() -> Result<ProviderKind> {
    if has_env("ANTHROPIC_API_KEY") {
        return Ok(ProviderKind::Anthropic);
    }
    if has_env("NVIDIA_API_KEY")
        || std::env::var("OPENAI_BASE_URL")
            .ok()
            .is_some_and(|u| u.contains("integrate.api.nvidia.com"))
    {
        return Ok(ProviderKind::Nim);
    }
    if has_env("OPENAI_API_KEY") {
        return Ok(ProviderKind::OpenAi);
    }
    Ok(ProviderKind::Ollama)
}

fn has_env(name: &str) -> bool {
    std::env::var(name).ok().is_some_and(|v| !v.is_empty())
}

/// Build an [`OpenAiConfig`] aimed at NVIDIA NIM's OpenAI-compatible
/// endpoint. Falls back to `OPENAI_API_KEY` if `NVIDIA_API_KEY` is unset
/// so users can swap providers by changing one env var.
#[cfg(feature = "http")]
fn nim_config_from_env() -> Result<OpenAiConfig> {
    let api_key = std::env::var("NVIDIA_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .map_err(|_| {
            LlmError::Misconfigured(
                "NVIDIA NIM selected but neither NVIDIA_API_KEY nor OPENAI_API_KEY is set".into(),
            )
        })?;
    let base_url = std::env::var("OPENAI_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://integrate.api.nvidia.com".to_owned());
    Ok(OpenAiConfig {
        api_key,
        base_url,
        http_timeout: std::time::Duration::from_secs(60),
        organization: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "http")]
    fn parse_known_kinds() {
        assert_eq!(
            ProviderKind::parse("anthropic").unwrap(),
            ProviderKind::Anthropic
        );
        assert_eq!(
            ProviderKind::parse("CLAUDE").unwrap(),
            ProviderKind::Anthropic
        );
        assert_eq!(ProviderKind::parse("openai").unwrap(), ProviderKind::OpenAi);
        assert_eq!(ProviderKind::parse("gpt").unwrap(), ProviderKind::OpenAi);
        assert_eq!(ProviderKind::parse("nim").unwrap(), ProviderKind::Nim);
        assert_eq!(ProviderKind::parse("nvidia").unwrap(), ProviderKind::Nim);
        assert_eq!(ProviderKind::parse("ollama").unwrap(), ProviderKind::Ollama);
        assert_eq!(ProviderKind::parse("local").unwrap(), ProviderKind::Ollama);
    }

    #[test]
    #[cfg(feature = "http")]
    fn parse_unknown_kind_errors() {
        let err = ProviderKind::parse("gemini").unwrap_err();
        assert!(matches!(err, LlmError::Misconfigured(_)));
    }

    #[test]
    fn each_kind_has_a_stable_name_and_default_model() {
        for k in [
            ProviderKind::Anthropic,
            ProviderKind::OpenAi,
            ProviderKind::Nim,
            ProviderKind::Ollama,
        ] {
            assert!(!k.as_str().is_empty());
            assert!(!k.default_model().is_empty());
        }
    }
}
