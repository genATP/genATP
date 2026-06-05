//! Cassette recorder — converts a completed `EventLog` into a signed
//! [`Cassette`].
//!
//! The recorder is the **write-path** of the cassette system. It:
//!
//! 1. Validates that the event log represents a **successful** run.
//! 2. Walks the log and pairs `Snapshot` events with the immediately
//!    following `ActionResult` events to build [`CassetteStep`]s.
//! 3. Computes the BLAKE3 hash chain across all steps.
//! 4. Signs the resulting cassette with the project signing key.
//! 5. Persists to `~/.genatp/cassettes/<slug>/versions/1.0.0.cassette`.
//!
//! ## Usage
//!
//! ```no_run
//! # use genatp_cassette::record::CassetteRecorder;
//! # use genatp_core::event_log::EventLog;
//! # fn example(log: &EventLog) -> genatp_cassette::error::Result<()> {
//! let recorder = CassetteRecorder::new("github-login", "Login to GitHub and find a repo");
//! let cassette = recorder.record(log)?;
//! # Ok(())
//! # }
//! ```

use chrono::Utc;
use ed25519_dalek::SigningKey;
use tracing::info;

use genatp_core::budget::BudgetSpend;
use genatp_core::event_log::{Event, EventLog};

use crate::error::{CassetteError, Result};
use crate::schema::{
    Cassette, CassetteHeader, CassetteStep, SCHEMA_VERSION, compute_step_hash,
    step_payload_for_hash,
};
use crate::signing;
use crate::storage;

/// Builder for recording a cassette from a completed event log.
#[derive(Debug)]
pub struct CassetteRecorder {
    slug: String,
    description: String,
}

impl CassetteRecorder {
    /// Create a new recorder targeting the given slug.
    #[must_use]
    pub fn new(slug: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            slug: slug.into(),
            description: description.into(),
        }
    }

    /// Record a cassette from a completed, **successful** event log.
    ///
    /// The event log must:
    /// 1. Pass hash-chain verification ([`EventLog::verify`]).
    /// 2. Contain a `RunFinished` event with outcome `"success"`.
    ///
    /// # Errors
    /// Returns [`CassetteError::RecordingFailed`] if the log is empty,
    /// verification fails, or the run did not succeed.
    pub fn record(&self, log: &EventLog) -> Result<Cassette> {
        // Validate log integrity.
        if log.is_empty() {
            return Err(CassetteError::RecordingFailed {
                reason: "event log is empty".into(),
            });
        }
        if let Some(bad_idx) = log.verify().map_err(|e| CassetteError::RecordingFailed {
            reason: format!("event log hash-chain verification failed: {e}"),
        })? {
            return Err(CassetteError::RecordingFailed {
                reason: format!("event log hash-chain broken at index {bad_idx}"),
            });
        }

        // Ensure the run succeeded.
        let run_finished = self.find_run_finished(log)?;

        // Extract task metadata from RunStarted.
        let (goal, start_url, model) = self.extract_run_metadata(log)?;

        // Build steps from the event log.
        let steps = self.build_steps(log)?;

        let tip_hash = steps.last().map_or(String::new(), |s| s.hash.clone());

        let header = CassetteHeader {
            schema_version: SCHEMA_VERSION,
            slug: self.slug.clone(),
            version: "1.0.0".into(),
            description: self.description.clone(),
            goal,
            start_url: start_url.clone(),
            model,
            recorded_at: Utc::now(),
            spend: run_finished.spend,
            step_count: steps.len() as u32,
            tip_hash,
            signer_public_key: String::new(), // filled in during save
            domains: self.extract_domains(log),
        };

        let cassette = Cassette { header, steps };

        // Verify our own hash chain before returning.
        cassette.verify_hash_chain()?;

        info!(
            slug = %self.slug,
            steps = cassette.steps.len(),
            "cassette recorded"
        );
        Ok(cassette)
    }

    /// Record and save to disk in one step. Ensures the signing key
    /// exists (generating if needed) and persists the cassette.
    ///
    /// # Errors
    /// Forwards recording, signing, and I/O errors.
    pub fn record_and_save(&self, log: &EventLog) -> Result<std::path::PathBuf> {
        let mut cassette = self.record(log)?;

        // Ensure we have a signing keypair.
        let verifying = signing::ensure_keypair()?;
        cassette.header.signer_public_key = signing::public_key_hex(&verifying);

        let signing_key = signing::load_signing_key()?;
        let root = storage::cassettes_dir()?;
        storage::save(&cassette, &root, &signing_key)
    }

    /// Record and save to a custom directory with a provided signing key.
    /// This is the testable variant that doesn't touch `~/.genatp/`.
    pub fn record_and_save_to(
        &self,
        log: &EventLog,
        root: &std::path::Path,
        signing_key: &SigningKey,
    ) -> Result<(Cassette, std::path::PathBuf)> {
        let mut cassette = self.record(log)?;
        cassette.header.signer_public_key = signing::public_key_hex(&signing_key.verifying_key());
        let path = storage::save(&cassette, root, signing_key)?;
        Ok((cassette, path))
    }

    // ── Private helpers ────────────────────────────────────────────────

    fn find_run_finished(&self, log: &EventLog) -> Result<RunFinishedData> {
        for entry in log.entries().iter().rev() {
            if let Event::RunFinished {
                outcome,
                steps,
                spend,
            } = &entry.event
            {
                if outcome != "success" {
                    return Err(CassetteError::RecordingFailed {
                        reason: format!("run outcome was `{outcome}`, not `success`"),
                    });
                }
                return Ok(RunFinishedData {
                    #[allow(clippy::clone_on_copy)]
                    spend: spend.clone(),
                    _steps: *steps,
                });
            }
        }
        Err(CassetteError::RecordingFailed {
            reason: "no RunFinished event found".into(),
        })
    }

    fn extract_run_metadata(&self, log: &EventLog) -> Result<(String, String, String)> {
        // Walk events for RunStarted and first LlmResponse (for model).
        let mut goal = String::new();
        let mut start_url = String::new();
        let mut model = String::new();

        for entry in log.entries() {
            match &entry.event {
                Event::RunStarted {
                    goal: g,
                    task_id: _,
                } => {
                    goal.clone_from(g);
                }
                Event::LlmResponse {
                    model: m,
                    provider: _,
                    usage: _,
                    finish_reason: _,
                } => {
                    if model.is_empty() {
                        model.clone_from(m);
                    }
                }
                // Capture the start URL from the first navigation action.
                Event::ActionResult {
                    action,
                    success: true,
                    message: _,
                } => {
                    if start_url.is_empty() {
                        if let genatp_core::action::Action::Navigate { url } = action {
                            start_url.clone_from(url);
                        }
                    }
                }
                _ => {}
            }
        }

        if goal.is_empty() {
            return Err(CassetteError::RecordingFailed {
                reason: "no RunStarted event found".into(),
            });
        }
        Ok((goal, start_url, model))
    }

    fn build_steps(&self, log: &EventLog) -> Result<Vec<CassetteStep>> {
        let entries = log.entries();
        let mut steps = Vec::new();
        let mut prev_hash = String::new();
        let mut current_snapshot: Option<SnapshotInfo> = None;

        for entry in entries {
            match &entry.event {
                Event::Snapshot {
                    url,
                    snapshot_hash,
                    token_estimate: _,
                } => {
                    current_snapshot = Some(SnapshotInfo {
                        url: url.clone(),
                        snapshot_hash: snapshot_hash.clone(),
                        at: entry.at,
                    });
                }
                Event::ActionResult {
                    action,
                    success,
                    message,
                } => {
                    // Skip the initial navigate ActionResult (before any
                    // snapshot is captured).
                    let Some(snap) = current_snapshot.take() else {
                        continue;
                    };

                    let seq = steps.len() as u32;
                    let mut step = CassetteStep {
                        seq,
                        at: snap.at,
                        url: snap.url,
                        snapshot_hash: snap.snapshot_hash,
                        action: action.clone(),
                        action_success: *success,
                        action_message: message.clone(),
                        parent_hash: prev_hash.clone(),
                        hash: String::new(),
                    };
                    let payload = step_payload_for_hash(&step);
                    step.hash = compute_step_hash(&prev_hash, &payload);
                    prev_hash.clone_from(&step.hash);
                    steps.push(step);
                }
                _ => {}
            }
        }

        if steps.is_empty() {
            return Err(CassetteError::RecordingFailed {
                reason: "no snapshot+action pairs found in event log".into(),
            });
        }
        Ok(steps)
    }

    fn extract_domains(&self, log: &EventLog) -> Vec<String> {
        let mut domains = std::collections::BTreeSet::new();
        for entry in log.entries() {
            match &entry.event {
                Event::Snapshot { url, .. }
                | Event::ActionResult {
                    action: genatp_core::action::Action::Navigate { url },
                    ..
                } => {
                    if let Ok(parsed) = url::Url::parse(url) {
                        if let Some(host) = parsed.host_str() {
                            domains.insert(host.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
        domains.into_iter().collect()
    }
}

struct RunFinishedData {
    spend: BudgetSpend,
    _steps: u32,
}

struct SnapshotInfo {
    url: String,
    snapshot_hash: String,
    at: chrono::DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use genatp_core::action::Action;
    use genatp_core::budget::BudgetSpend;
    use genatp_core::event_log::{Event, EventLog};
    use genatp_llm::Usage;

    /// Build a realistic event log representing a successful 2-step run.
    fn build_successful_log() -> EventLog {
        let mut log = EventLog::in_memory();

        log.append(Event::RunStarted {
            task_id: "test-task".into(),
            goal: "Click the button".into(),
        })
        .unwrap();

        // Initial navigation.
        log.append(Event::ActionRequested {
            action: Action::Navigate {
                url: "https://example.com".into(),
            },
        })
        .unwrap();
        log.append(Event::ActionResult {
            action: Action::Navigate {
                url: "https://example.com".into(),
            },
            success: true,
            message: Some("navigated".into()),
        })
        .unwrap();

        // Step 1: Snapshot + LLM + Click.
        log.append(Event::Snapshot {
            url: "https://example.com".into(),
            snapshot_hash: "snap_hash_1".into(),
            token_estimate: 200,
        })
        .unwrap();
        log.append(Event::LlmRequest {
            provider: "mock".into(),
            model: "gpt-4".into(),
            prompt_messages: 3,
        })
        .unwrap();
        log.append(Event::LlmResponse {
            provider: "mock".into(),
            model: "gpt-4".into(),
            usage: Usage {
                prompt_tokens: 100,
                completion_tokens: 20,
                total_tokens: 120,
            },
            finish_reason: "tool_calls".into(),
        })
        .unwrap();
        log.append(Event::ActionRequested {
            action: Action::Click {
                r#ref: "@e3".into(),
            },
        })
        .unwrap();
        log.append(Event::ActionResult {
            action: Action::Click {
                r#ref: "@e3".into(),
            },
            success: true,
            message: Some("clicked".into()),
        })
        .unwrap();

        // Step 2: Snapshot + LLM + Finish.
        log.append(Event::Snapshot {
            url: "https://example.com/result".into(),
            snapshot_hash: "snap_hash_2".into(),
            token_estimate: 150,
        })
        .unwrap();
        log.append(Event::LlmRequest {
            provider: "mock".into(),
            model: "gpt-4".into(),
            prompt_messages: 5,
        })
        .unwrap();
        log.append(Event::LlmResponse {
            provider: "mock".into(),
            model: "gpt-4".into(),
            usage: Usage {
                prompt_tokens: 150,
                completion_tokens: 30,
                total_tokens: 180,
            },
            finish_reason: "tool_calls".into(),
        })
        .unwrap();
        log.append(Event::ActionRequested {
            action: Action::Finish {
                summary: "Done!".into(),
            },
        })
        .unwrap();
        log.append(Event::ActionResult {
            action: Action::Finish {
                summary: "Done!".into(),
            },
            success: true,
            message: None,
        })
        .unwrap();

        // Run finished.
        log.append(Event::RunFinished {
            outcome: "success".into(),
            steps: 2,
            spend: BudgetSpend::default(),
        })
        .unwrap();

        log
    }

    #[test]
    fn record_from_successful_log() {
        let log = build_successful_log();
        let recorder = CassetteRecorder::new("test-record", "Test recording");
        let cassette = recorder.record(&log).unwrap();

        assert_eq!(cassette.header.slug, "test-record");
        assert_eq!(cassette.header.version, "1.0.0");
        assert_eq!(cassette.header.goal, "Click the button");
        assert_eq!(cassette.header.start_url, "https://example.com");
        assert_eq!(cassette.header.model, "gpt-4");
        assert_eq!(cassette.header.schema_version, SCHEMA_VERSION);
        assert_eq!(cassette.steps.len(), 2);

        // Step 0 is the click.
        assert!(matches!(cassette.steps[0].action, Action::Click { .. }));
        assert!(cassette.steps[0].parent_hash.is_empty());

        // Step 1 is the finish.
        assert!(matches!(cassette.steps[1].action, Action::Finish { .. }));
        assert_eq!(cassette.steps[1].parent_hash, cassette.steps[0].hash);

        // Hash chain is valid.
        cassette.verify_hash_chain().unwrap();
    }

    #[test]
    fn record_rejects_failed_run() {
        let mut log = EventLog::in_memory();
        log.append(Event::RunStarted {
            task_id: "t".into(),
            goal: "g".into(),
        })
        .unwrap();
        log.append(Event::RunFinished {
            outcome: "aborted".into(),
            steps: 0,
            spend: BudgetSpend::default(),
        })
        .unwrap();

        let recorder = CassetteRecorder::new("fail", "should fail");
        let err = recorder.record(&log).unwrap_err();
        assert!(matches!(err, CassetteError::RecordingFailed { .. }));
    }

    #[test]
    fn record_rejects_empty_log() {
        let log = EventLog::in_memory();
        let recorder = CassetteRecorder::new("empty", "should fail");
        let err = recorder.record(&log).unwrap_err();
        assert!(matches!(err, CassetteError::RecordingFailed { .. }));
    }

    #[test]
    fn record_and_save_roundtrip() {
        let log = build_successful_log();
        let recorder = CassetteRecorder::new("roundtrip-test", "roundtrip");

        let tmp = tempfile::tempdir().unwrap();
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let verifying_key = signing_key.verifying_key();

        let (cassette, path) = recorder
            .record_and_save_to(&log, tmp.path(), &signing_key)
            .unwrap();

        // Load it back.
        let loaded = storage::load(&path, &verifying_key).unwrap();
        assert_eq!(loaded.header.slug, cassette.header.slug);
        assert_eq!(loaded.steps.len(), cassette.steps.len());

        // Verify the cassette is compact.
        let size = std::fs::metadata(&path).unwrap().len();
        assert!(size < 10_000, "cassette should be < 10 KB, got {size}");
    }

    #[test]
    fn domains_extracted_from_log() {
        let log = build_successful_log();
        let recorder = CassetteRecorder::new("domains", "test domains");
        let cassette = recorder.record(&log).unwrap();
        assert!(cassette.header.domains.contains(&"example.com".to_string()));
    }

    #[test]
    fn hash_chain_tip_matches_header() {
        let log = build_successful_log();
        let recorder = CassetteRecorder::new("tip", "test tip");
        let cassette = recorder.record(&log).unwrap();
        assert_eq!(
            cassette.header.tip_hash,
            cassette.steps.last().unwrap().hash
        );
    }
}
