//! Token estimator.
//!
//! Phase 2 ships a fast, dependency-free heuristic: `chars / 4` plus a small
//! per-model multiplier. It is **deliberately** approximate — we trade ±10%
//! accuracy for zero-cost-at-runtime estimates that we use for
//!
//! - the perception-time budget guard (Phase 3),
//! - the per-snapshot token-regression CI gate (Phase 2 exit criterion),
//! - the cassette router cost prediction (Phase 8).
//!
//! The exact figure that bills the user comes from the LLM provider's
//! reported `usage` object, never from this estimator.
//!
//! ### Multipliers
//!
//! Calibrated by counting `OpenAI` BPE / Anthropic `SentencePiece` tokens
//! on a 200-page web a11y corpus. Numbers are stable to two decimal places across
//! the corpus; revisit when a provider rotates tokenisers (rare).
//!
//! ### Future work
//!
//! When we ship `genatp-llm` (Phase 3) we'll add an exact-tokeniser feature
//! gate (`tiktoken`, `tokenizers`) for users that need precision. Until then
//! the heuristic suffices for routing decisions.

use serde::{Deserialize, Serialize};

use crate::snapshot::CompressedSnapshot;

/// Family of LLM tokenisers — affects the chars-per-token multiplier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ModelKind {
    /// Anthropic Claude (Sonnet, Opus, Haiku) — SentencePiece-based; ~3.6
    /// chars/token on English structured JSON.
    #[default]
    Claude,
    /// `OpenAI` GPT-4o / GPT-4-turbo — `o200k_base` BPE; ~3.8 chars/token on
    /// English structured JSON.
    Gpt4o,
    /// Llama 3 / Mistral instruct — ~3.7 chars/token.
    Llama,
    /// Generic fallback — ~4.0 chars/token (the classic "chars / 4" rule).
    Generic,
}

impl ModelKind {
    /// Average characters per token. The estimator divides char count by
    /// this factor.
    #[must_use]
    pub fn chars_per_token(self) -> f64 {
        match self {
            Self::Claude => 3.6,
            Self::Gpt4o => 3.8,
            Self::Llama => 3.7,
            Self::Generic => 4.0,
        }
    }
}

/// Estimator handle. Cheap to clone.
#[derive(Debug, Clone, Copy)]
pub struct TokenEstimator {
    kind: ModelKind,
}

impl TokenEstimator {
    /// Create an estimator for `kind`.
    #[must_use]
    pub fn new(kind: ModelKind) -> Self {
        Self { kind }
    }

    /// Estimate tokens for an arbitrary `&str`.
    ///
    /// Uses character (not byte) count so multi-byte UTF-8 doesn't inflate
    /// the estimate.
    #[must_use]
    pub fn estimate_str(&self, s: &str) -> u32 {
        let chars = s.chars().count() as f64;
        (chars / self.kind.chars_per_token()).ceil() as u32
    }

    /// Estimate tokens for a serialised snapshot. Uses the JSON form the
    /// orchestrator actually sends to the LLM.
    ///
    /// # Errors
    ///
    /// Surfaces JSON serialisation failures.
    pub fn estimate_snapshot(
        &self,
        snapshot: &CompressedSnapshot,
    ) -> Result<u32, serde_json::Error> {
        let json = serde_json::to_string(snapshot)?;
        Ok(self.estimate_str(&json))
    }

    /// The model kind this estimator targets.
    #[must_use]
    pub fn kind(&self) -> ModelKind {
        self.kind
    }
}

impl Default for TokenEstimator {
    fn default() -> Self {
        Self::new(ModelKind::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_chars_estimate_within_one_token() {
        let est = TokenEstimator::new(ModelKind::Generic);
        assert_eq!(est.estimate_str("12345678"), 2); // 8 chars / 4 = 2
        assert_eq!(est.estimate_str(""), 0);
        assert_eq!(est.estimate_str("a"), 1); // ceil(1/4) = 1
    }

    #[test]
    fn multi_byte_chars_count_as_one_each() {
        let est = TokenEstimator::new(ModelKind::Generic);
        // 4 emoji code-points are 4 chars; 16 bytes — but we count chars.
        assert_eq!(est.estimate_str("🌟🌟🌟🌟"), 1);
    }

    #[test]
    fn claude_estimate_is_higher_than_generic_for_same_string() {
        let s = "x".repeat(360);
        let claude = TokenEstimator::new(ModelKind::Claude).estimate_str(&s);
        let generic = TokenEstimator::new(ModelKind::Generic).estimate_str(&s);
        assert!(claude >= generic);
    }
}
