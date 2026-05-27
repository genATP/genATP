//! Concrete provider implementations.
//!
//! ### Implemented in Phase 3
//!
//! - [`anthropic::AnthropicProvider`] — Claude (Messages API).
//! - [`openai::OpenAiProvider`] — GPT-4o family (Chat Completions API).
//! - [`ollama::OllamaProvider`] — local Ollama server (`/api/chat`).
//!
//! ### Documented placeholders
//!
//! Google (Gemini), Mistral, and vLLM share wire shapes with the providers
//! above. They are intentionally deferred to **v0.4** to keep the Phase 3
//! review surface small. Add them by mirroring the Anthropic adapter:
//! request mapping, response mapping, error classification, integration
//! test against a VCR cassette.

#[cfg(feature = "http")]
pub mod anthropic;
#[cfg(feature = "http")]
pub mod ollama;
#[cfg(feature = "http")]
pub mod openai;

pub mod placeholders;
