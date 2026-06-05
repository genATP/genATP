//! Cassette v1 schema — the core data structures for Action Cassettes.
//!
//! A cassette is a signed, versioned recording of a successful agent run.
//! The on-disk format is:
//!
//! ```text
//! ┌──────────────────────────────────────────────────────┐
//! │  zstd( canonical_json( Cassette ) )                  │
//! │  + 64-byte ed25519 signature appended after payload  │
//! └──────────────────────────────────────────────────────┘
//! ```
//!
//! The signature covers the **zstd-compressed** payload (not the raw JSON),
//! so verification is a single `decompress → hash → verify` pass.
//!
//! ## Design decisions
//!
//! - **JSON (JCS) + zstd**: human-inspectable after decompression; zstd
//!   yields excellent ratios on repetitive JSON (typically 5–10×).
//! - **Ed25519 signatures**: fast (< 1 ms), well-audited (`ed25519-dalek`),
//!   64-byte signatures, deterministic.
//! - **Hash-chained steps**: `step[n].hash = BLAKE3(step[n-1].hash || step[n].payload)`.
//!   Tamper with one step and every subsequent hash breaks.
//! - **Semver versioning**: enables the self-heal engine (Phase 8) to bump
//!   the cassette version on drift repair.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use genatp_core::action::Action;
use genatp_core::budget::BudgetSpend;

/// Schema version. Bump only for breaking format changes.
pub const SCHEMA_VERSION: u32 = 1;

/// File extension used for cassette files.
pub const CASSETTE_EXT: &str = "cassette";

/// Metadata for a cassette — who recorded it, when, and what it does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CassetteHeader {
    /// Schema version — always [`SCHEMA_VERSION`] for forward compat checks.
    pub schema_version: u32,
    /// Human-readable slug (e.g. `"github-login-repo-lookup"`).
    pub slug: String,
    /// Semver string (e.g. `"1.0.0"`). The recorder always creates `1.0.0`;
    /// the self-heal engine bumps the minor on drift repair.
    pub version: String,
    /// Free-form description of what this cassette automates.
    pub description: String,
    /// The goal string from the original [`Task`].
    pub goal: String,
    /// Starting URL the agent navigated to.
    pub start_url: String,
    /// LLM model used during the recording run.
    pub model: String,
    /// UTC timestamp when the cassette was recorded.
    pub recorded_at: DateTime<Utc>,
    /// Budget spend at the end of the recording run.
    pub spend: BudgetSpend,
    /// Total number of steps in the cassette.
    pub step_count: u32,
    /// Hex-encoded BLAKE3 hash of the last step (the "tip" of the chain).
    pub tip_hash: String,
    /// Hex-encoded ed25519 public key of the signer.
    pub signer_public_key: String,
    /// List of domains the cassette accesses (for vault capability checks).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domains: Vec<String>,
}

/// One step in a cassette — a snapshot + action pair.
///
/// The hash chain links steps together:
/// ```text
/// step[0].hash = BLAKE3("" || canonical_json(step[0].payload))
/// step[n].hash = BLAKE3(step[n-1].hash || canonical_json(step[n].payload))
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CassetteStep {
    /// 0-based step index.
    pub seq: u32,
    /// UTC timestamp of this step.
    pub at: DateTime<Utc>,
    /// Current page URL at this step.
    pub url: String,
    /// BLAKE3 hash of the compressed snapshot at this step (hex).
    pub snapshot_hash: String,
    /// The action dispatched at this step.
    pub action: Action,
    /// Whether the action succeeded.
    pub action_success: bool,
    /// Optional human-readable result message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_message: Option<String>,
    /// Hash of the *previous* step's hash (empty string for step 0).
    pub parent_hash: String,
    /// `BLAKE3(parent_hash || canonical_json(step_payload))` — hex encoded.
    pub hash: String,
}

/// The complete cassette: header + ordered steps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cassette {
    pub header: CassetteHeader,
    pub steps: Vec<CassetteStep>,
}

impl Cassette {
    /// Verify the internal hash chain. Returns `Ok(())` if all steps are
    /// consistent, or the index of the first broken step.
    ///
    /// # Errors
    /// Returns [`CassetteError::HashChainBroken`] on the first mismatch.
    pub fn verify_hash_chain(&self) -> crate::error::Result<()> {
        let mut prev_hash = String::new();
        for step in &self.steps {
            let payload = step_payload_for_hash(step);
            let computed = compute_step_hash(&prev_hash, &payload);
            if computed != step.hash {
                return Err(crate::error::CassetteError::HashChainBroken { step: step.seq });
            }
            prev_hash = step.hash.clone();
        }
        // Verify tip matches header.
        if !self.steps.is_empty() && prev_hash != self.header.tip_hash {
            return Err(crate::error::CassetteError::HashChainBroken {
                step: self.header.step_count.saturating_sub(1),
            });
        }
        Ok(())
    }

    /// Number of steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// True if the cassette has no steps.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// Compute the canonical payload bytes for a step's hash. This is a
/// deterministic JSON blob of the step's semantic content (excluding the
/// hash itself and the parent_hash, since those are part of the chain
/// computation).
#[must_use]
pub fn step_payload_for_hash(step: &CassetteStep) -> Vec<u8> {
    // We build a canonical structure containing only the fields that
    // contribute to the hash. This is intentionally NOT the full step
    // serialization — the hash and parent_hash are excluded.
    let payload = serde_json::json!({
        "seq": step.seq,
        "at": step.at.to_rfc3339(),
        "url": step.url,
        "snapshot_hash": step.snapshot_hash,
        "action": step.action,
        "action_success": step.action_success,
        "action_message": step.action_message,
    });
    crate::canon::canonical_json(&payload)
}

/// Compute `BLAKE3(prev_hash_bytes || payload_bytes)` → hex string.
#[must_use]
pub fn compute_step_hash(prev_hash: &str, payload: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(payload);
    hex::encode(hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_step(seq: u32, prev_hash: &str) -> CassetteStep {
        let action = Action::Navigate {
            url: "https://example.com".into(),
        };
        let payload_step = CassetteStep {
            seq,
            at: chrono::Utc::now(),
            url: "https://example.com".into(),
            snapshot_hash: "abc123".into(),
            action,
            action_success: true,
            action_message: None,
            parent_hash: prev_hash.into(),
            hash: String::new(), // placeholder
        };
        let payload = step_payload_for_hash(&payload_step);
        let hash = compute_step_hash(prev_hash, &payload);
        CassetteStep {
            hash,
            ..payload_step
        }
    }

    #[test]
    fn hash_chain_verify_succeeds_on_valid_cassette() {
        let step0 = sample_step(0, "");
        let step1 = sample_step(1, &step0.hash);
        let cassette = Cassette {
            header: CassetteHeader {
                schema_version: SCHEMA_VERSION,
                slug: "test".into(),
                version: "1.0.0".into(),
                description: "test cassette".into(),
                goal: "test goal".into(),
                start_url: "https://example.com".into(),
                model: "mock".into(),
                recorded_at: chrono::Utc::now(),
                spend: BudgetSpend::default(),
                step_count: 2,
                tip_hash: step1.hash.clone(),
                signer_public_key: String::new(),
                domains: vec![],
            },
            steps: vec![step0, step1],
        };
        cassette.verify_hash_chain().unwrap();
    }

    #[test]
    fn hash_chain_detects_tampered_step() {
        let step0 = sample_step(0, "");
        let mut step1 = sample_step(1, &step0.hash);
        // Tamper with step1's URL.
        step1.url = "https://evil.com".into();
        let cassette = Cassette {
            header: CassetteHeader {
                schema_version: SCHEMA_VERSION,
                slug: "test".into(),
                version: "1.0.0".into(),
                description: "test cassette".into(),
                goal: "test goal".into(),
                start_url: "https://example.com".into(),
                model: "mock".into(),
                recorded_at: chrono::Utc::now(),
                spend: BudgetSpend::default(),
                step_count: 2,
                tip_hash: step1.hash.clone(),
                signer_public_key: String::new(),
                domains: vec![],
            },
            steps: vec![step0, step1],
        };
        let err = cassette.verify_hash_chain().unwrap_err();
        assert!(
            matches!(
                err,
                crate::error::CassetteError::HashChainBroken { step: 1 }
            ),
            "expected broken at step 1, got {err:?}"
        );
    }

    #[test]
    fn step_payload_is_deterministic() {
        let step = sample_step(0, "");
        let p1 = step_payload_for_hash(&step);
        let p2 = step_payload_for_hash(&step);
        assert_eq!(p1, p2, "payload must be deterministic");
    }

    #[test]
    fn empty_cassette_verifies() {
        let cassette = Cassette {
            header: CassetteHeader {
                schema_version: SCHEMA_VERSION,
                slug: "empty".into(),
                version: "1.0.0".into(),
                description: String::new(),
                goal: String::new(),
                start_url: String::new(),
                model: String::new(),
                recorded_at: chrono::Utc::now(),
                spend: BudgetSpend::default(),
                step_count: 0,
                tip_hash: String::new(),
                signer_public_key: String::new(),
                domains: vec![],
            },
            steps: vec![],
        };
        cassette.verify_hash_chain().unwrap();
    }

    #[test]
    fn serde_roundtrip() {
        let step = sample_step(0, "");
        let cassette = Cassette {
            header: CassetteHeader {
                schema_version: SCHEMA_VERSION,
                slug: "test".into(),
                version: "1.0.0".into(),
                description: "test".into(),
                goal: "goal".into(),
                start_url: "https://example.com".into(),
                model: "mock".into(),
                recorded_at: chrono::Utc::now(),
                spend: BudgetSpend::default(),
                step_count: 1,
                tip_hash: step.hash.clone(),
                signer_public_key: String::new(),
                domains: vec!["example.com".into()],
            },
            steps: vec![step],
        };
        let json = serde_json::to_string(&cassette).unwrap();
        let back: Cassette = serde_json::from_str(&json).unwrap();
        assert_eq!(cassette, back);
    }
}
