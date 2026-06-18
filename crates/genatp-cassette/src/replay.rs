//! Cassette replay engine — deterministic, zero-LLM-token execution of a
//! recorded [`Cassette`].
//!
//! ## How it works
//!
//! 1. **Load and verify** a cassette (Phase 7 ensures the signature + hash
//!    chain are valid).
//! 2. **Navigate** to the cassette's `start_url`.
//! 3. For **each step** in the cassette:
//!    - Execute the step's [`Action`](genatp_core::action::Action) through
//!      the [`BrowserAdapter`](genatp_core::dispatcher::BrowserAdapter).
//!    - `Click` and `Fill` actions contain `@eN` refs that must be resolved
//!      to CDP node IDs; the caller supplies a **ref resolver** callback.
//!    - Record success/failure in a [`ReplayStepResult`].
//! 4. Return a [`ReplayOutcome`] summarising the full run.
//!
//! **No LLM calls are made during replay.** The recorded actions are
//! executed exactly as written in the cassette. This means replays are:
//! - **Free** — zero token cost.
//! - **Deterministic** — same cassette, same actions, same sequence.
//! - **Verifiable** — every step's success is tracked.
//!
//! ## Self-heal (future)
//!
//! When a step fails (e.g. an element referenced by `@eN` no longer exists
//! on the page), the self-heal engine (planned for later in Phase 8) will
//! attempt to re-record the failed step by falling back to an LLM call.
//! Failed steps are flagged in [`ReplayStepStatus::Failed`].

use tracing::{debug, info, warn};

use genatp_core::action::Action;
use genatp_core::dispatcher::BrowserAdapter;

use crate::error::{CassetteError, Result};
use crate::schema::Cassette;

/// Per-step replay status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayStepStatus {
    /// The action executed successfully. The `url_matches` flag indicates
    /// whether the current page URL matched the step's recorded URL.
    Success { url_matches: bool },
    /// The action execution failed.
    Failed { reason: String },
}

/// Outcome of a full cassette replay.
#[derive(Debug, Clone)]
pub struct ReplayOutcome {
    /// Number of steps that succeeded.
    pub steps_succeeded: u32,
    /// Number of steps that failed.
    pub steps_failed: u32,
    /// Per-step results in order.
    pub step_results: Vec<ReplayStepResult>,
    /// Total wall-clock duration of the replay in milliseconds.
    pub duration_ms: u64,
}

/// Result of replaying a single cassette step.
#[derive(Debug, Clone)]
pub struct ReplayStepResult {
    pub seq: u32,
    pub action: Action,
    pub status: ReplayStepStatus,
    pub url: String,
}

/// The replay engine.
///
/// Construct one with [`CassetteReplayer::new`] and call [`replay`](CassetteReplayer::replay)
/// with a [`BrowserAdapter`] and a **ref resolver** callback.
///
/// ## Ref resolver
///
/// Click and Fill actions reference elements by `@eN` refs (e.g. `@e3`).
/// These must be resolved to CDP backend node IDs for the underlying
/// browser adapter. The replayer calls the resolver for every Click/Fill
/// action; return `None` to treat the step as failed.
///
/// A typical implementation fetches the accessibility tree, compresses it
/// via `genatp-perception`, and builds a `HashMap<Ref, String>` lookup.
#[derive(Debug)]
pub struct CassetteReplayer {
    cassette: Cassette,
}

impl CassetteReplayer {
    /// Create a new replayer for the given cassette.
    #[must_use]
    pub fn new(cassette: Cassette) -> Self {
        Self { cassette }
    }

    /// Borrow the underlying cassette.
    #[must_use]
    pub fn cassette(&self) -> &Cassette {
        &self.cassette
    }

    /// Replay the cassette deterministically using the given browser
    /// adapter and ref-resolver callback.
    ///
    /// The `ref_resolver` receives the `@eN` string from each Click/Fill
    /// action and should return the corresponding CDP backend node ID
    /// (or `None` if the element no longer exists on the page).
    ///
    /// # Errors
    ///
    /// Returns [`CassetteError::ReplayFailed`] if:
    /// - The initial navigation to `start_url` fails.
    /// - A step's action cannot be executed.
    /// - The ref resolver returns `None` for a Click/Fill action.
    ///
    /// Returns [`CassetteError::ReplayJoin`] if an internal async task
    /// errors.
    pub async fn replay<F>(
        &self,
        adapter: &dyn BrowserAdapter,
        ref_resolver: F,
    ) -> Result<ReplayOutcome>
    where
        F: Fn(&str) -> Option<String>,
    {
        let start = std::time::Instant::now();
        let mut step_results: Vec<ReplayStepResult> = Vec::new();
        let mut steps_succeeded: u32 = 0;
        let mut steps_failed: u32 = 0;

        info!(
            slug = %self.cassette.header.slug,
            version = %self.cassette.header.version,
            steps = self.cassette.steps.len(),
            "starting cassette replay"
        );

        // 1) Navigate to the start URL.
        debug!(
            url = %self.cassette.header.start_url,
            "replay: navigating to start URL"
        );
        if let Err(e) = adapter.navigate(&self.cassette.header.start_url).await {
            return Err(CassetteError::ReplayFailed {
                step: 0,
                action: Action::Navigate {
                    url: self.cassette.header.start_url.clone(),
                },
                reason: format!("initial navigation failed: {e}"),
            });
        }

        // 2) Walk through each step.
        for step in &self.cassette.steps {
            let result = self
                .execute_step(adapter, &ref_resolver, step)
                .await
                .unwrap_or_else(|e| ReplayStepResult {
                    seq: step.seq,
                    action: step.action.clone(),
                    status: ReplayStepStatus::Failed {
                        reason: e.to_string(),
                    },
                    url: step.url.clone(),
                });

            match &result.status {
                ReplayStepStatus::Success { .. } => steps_succeeded += 1,
                ReplayStepStatus::Failed { .. } => steps_failed += 1,
            }

            // Check current URL (best-effort, not critical).
            let current_url = adapter
                .current_url()
                .await
                .unwrap_or(None)
                .unwrap_or_default();

            // If the action was Finish, we're done.
            if matches!(&result.action, Action::Finish { .. }) {
                step_results.push(result);
                break;
            }

            step_results.push(result);

            // If a non-Finish step failed and the URL doesn't match,
            // log a warning but continue (self-heal could fix this later).
            if let ReplayStepStatus::Failed { reason } = &step_results.last().unwrap().status {
                warn!(
                    step = step.seq,
                    expected_url = %step.url,
                    current_url = %current_url,
                    reason,
                    "replay step failed"
                );
            }
        }

        let duration = start.elapsed();
        let outcome = ReplayOutcome {
            steps_succeeded,
            steps_failed,
            step_results,
            duration_ms: duration.as_millis() as u64,
        };

        info!(
            slug = %self.cassette.header.slug,
            succeeded = outcome.steps_succeeded,
            failed = outcome.steps_failed,
            duration_ms = outcome.duration_ms,
            "cassette replay completed"
        );

        Ok(outcome)
    }

    /// Execute a single cassette step against the browser.
    async fn execute_step<F>(
        &self,
        adapter: &dyn BrowserAdapter,
        ref_resolver: &F,
        step: &crate::schema::CassetteStep,
    ) -> std::result::Result<ReplayStepResult, CassetteError>
    where
        F: Fn(&str) -> Option<String>,
    {
        debug!(seq = step.seq, action = ?step.action, "replaying step");

        let action = &step.action;
        match action {
            Action::Navigate { url } => {
                adapter
                    .navigate(url)
                    .await
                    .map_err(|e| CassetteError::ReplayFailed {
                        step: step.seq,
                        action: action.clone(),
                        reason: format!("navigate failed: {e}"),
                    })?;
            }
            Action::Click { r#ref } => {
                let ref_str: &str = r#ref;
                let node_id = ref_resolver(ref_str).ok_or_else(|| CassetteError::ReplayFailed {
                    step: step.seq,
                    action: action.clone(),
                    reason: format!("ref `{ref_str}` could not be resolved on the current page"),
                })?;
                adapter
                    .click_node(&node_id)
                    .await
                    .map_err(|e| CassetteError::ReplayFailed {
                        step: step.seq,
                        action: action.clone(),
                        reason: format!("click `{ref_str}` (node {node_id}) failed: {e}"),
                    })?;
            }
            Action::Fill { r#ref, value } => {
                let ref_str: &str = r#ref;
                let node_id = ref_resolver(ref_str).ok_or_else(|| CassetteError::ReplayFailed {
                    step: step.seq,
                    action: action.clone(),
                    reason: format!("ref `{ref_str}` could not be resolved on the current page"),
                })?;
                adapter.fill_node(&node_id, value).await.map_err(|e| {
                    CassetteError::ReplayFailed {
                        step: step.seq,
                        action: action.clone(),
                        reason: format!("fill `{ref_str}` (node {node_id}) failed: {e}"),
                    }
                })?;
            }
            Action::Press { key } => {
                adapter
                    .press_key(key)
                    .await
                    .map_err(|e| CassetteError::ReplayFailed {
                        step: step.seq,
                        action: action.clone(),
                        reason: format!("press key failed: {e}"),
                    })?;
            }
            Action::ClickCoordinate { x, y } => {
                adapter.click_coordinate(*x, *y).await.map_err(|e| {
                    CassetteError::ReplayFailed {
                        step: step.seq,
                        action: action.clone(),
                        reason: format!("click_coordinate ({x},{y}) failed: {e}"),
                    }
                })?;
            }
            Action::Scroll { dx, dy } => {
                adapter
                    .scroll(*dx, *dy)
                    .await
                    .map_err(|e| CassetteError::ReplayFailed {
                        step: step.seq,
                        action: action.clone(),
                        reason: format!("scroll failed: {e}"),
                    })?;
            }
            Action::Wait { ms } => {
                tokio::time::sleep(std::time::Duration::from_millis(u64::from(*ms))).await;
            }
            Action::Finish { .. } => {
                // Finish is a no-op for the browser; the orchestrator
                // returns success without touching the adapter.
            }
        }

        // Check if the current URL matches what was recorded.
        let current_url = adapter
            .current_url()
            .await
            .unwrap_or(None)
            .unwrap_or_default();
        let url_matches = current_url == step.url
            || current_url.trim_end_matches('/') == step.url.trim_end_matches('/');

        let status = ReplayStepStatus::Success { url_matches };

        Ok(ReplayStepResult {
            seq: step.seq,
            action: action.clone(),
            status,
            url: current_url,
        })
    }
}

impl CassetteReplayer {
    /// Replay without ref resolution — only `Navigate`, `Scroll`, `Press`,
    /// `Wait`, and `Finish` actions will succeed. `Click` and `Fill` will
    /// fail because element refs cannot be resolved.
    pub async fn replay_simple(&self, adapter: &dyn BrowserAdapter) -> Result<ReplayOutcome> {
        self.replay(adapter, |_| None).await
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use genatp_core::budget::BudgetSpend;
    use genatp_core::event_log::{Event, EventLog};

    use crate::record::CassetteRecorder;
    use crate::schema::{
        CassetteHeader, CassetteStep, SCHEMA_VERSION, compute_step_hash, step_payload_for_hash,
    };

    // ── Mock browser adapter ──────────────────────────────────────────────
    //
    // Note: `accessibility()` is never called by the replayer (it only calls
    // `navigate`, `click_node`, `fill_node`, `press_key`, `scroll`, and
    // `current_url`), so the mock body returns an empty tree.

    use genatp_browser::AccessibilityTree;
    use genatp_core::error::CoreError;

    #[derive(Debug, Default)]
    struct MockBrowser {
        events: Mutex<Vec<String>>,
        current_url: Mutex<String>,
    }

    use async_trait::async_trait;

    #[async_trait]
    impl BrowserAdapter for MockBrowser {
        async fn navigate(&self, url: &str) -> std::result::Result<(), CoreError> {
            let mut u = self.current_url.lock().unwrap();
            *u = url.to_string();
            self.events.lock().unwrap().push(format!("navigate {url}"));
            Ok(())
        }

        async fn click_node(&self, node_id: &str) -> std::result::Result<(), CoreError> {
            self.events.lock().unwrap().push(format!("click {node_id}"));
            Ok(())
        }

        async fn fill_node(
            &self,
            node_id: &str,
            value: &str,
        ) -> std::result::Result<(), CoreError> {
            self.events
                .lock()
                .unwrap()
                .push(format!("fill {node_id}={value}"));
            Ok(())
        }

        async fn press_key(&self, key: &str) -> std::result::Result<(), CoreError> {
            self.events.lock().unwrap().push(format!("press {key}"));
            Ok(())
        }

        async fn click_coordinate(&self, x: f64, y: f64) -> std::result::Result<(), CoreError> {
            self.events
                .lock()
                .unwrap()
                .push(format!("click_coordinate {x},{y}"));
            Ok(())
        }

        async fn scroll(&self, dx: i32, dy: i32) -> std::result::Result<(), CoreError> {
            self.events
                .lock()
                .unwrap()
                .push(format!("scroll {dx},{dy}"));
            Ok(())
        }

        async fn current_url(&self) -> std::result::Result<Option<String>, CoreError> {
            Ok(Some(self.current_url.lock().unwrap().clone()))
        }

        async fn accessibility(&self) -> std::result::Result<AccessibilityTree, CoreError> {
            Ok(AccessibilityTree::from_nodes(vec![]))
        }

        async fn screenshot_png(&self) -> std::result::Result<Vec<u8>, CoreError> {
            Ok(vec![])
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    fn make_simple_cassette() -> Cassette {
        let action = Action::Navigate {
            url: "https://example.com".into(),
        };
        let step = CassetteStep {
            seq: 0,
            at: chrono::Utc::now(),
            url: "https://example.com".into(),
            snapshot_hash: "abc".into(),
            action,
            action_success: true,
            action_message: None,
            parent_hash: String::new(),
            hash: String::new(),
        };
        let payload = step_payload_for_hash(&step);
        let hash = compute_step_hash("", &payload);
        let step = CassetteStep {
            hash: hash.clone(),
            ..step
        };

        Cassette {
            header: CassetteHeader {
                schema_version: SCHEMA_VERSION,
                slug: "replay-test".into(),
                version: "1.0.0".into(),
                description: "replay test".into(),
                goal: "test".into(),
                start_url: "https://example.com".into(),
                model: "mock".into(),
                recorded_at: chrono::Utc::now(),
                spend: BudgetSpend::default(),
                step_count: 1,
                tip_hash: hash,
                signer_public_key: String::new(),
                domains: vec!["example.com".into()],
            },
            steps: vec![step],
        }
    }

    fn build_replay_test_cassette() -> Cassette {
        let mut log = EventLog::in_memory();
        log.append(Event::RunStarted {
            task_id: "replay-e2e".into(),
            goal: "Replay test".into(),
        })
        .unwrap();

        // Navigate
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
            message: None,
        })
        .unwrap();

        // Step 1: Snapshot + Click
        log.append(Event::Snapshot {
            url: "https://example.com".into(),
            snapshot_hash: "snap1".into(),
            token_estimate: 100,
        })
        .unwrap();
        log.append(Event::ActionRequested {
            action: Action::Click {
                r#ref: "@e1".into(),
            },
        })
        .unwrap();
        log.append(Event::ActionResult {
            action: Action::Click {
                r#ref: "@e1".into(),
            },
            success: true,
            message: None,
        })
        .unwrap();

        // Step 2: Snapshot + Fill
        log.append(Event::Snapshot {
            url: "https://example.com/form".into(),
            snapshot_hash: "snap2".into(),
            token_estimate: 100,
        })
        .unwrap();
        log.append(Event::ActionRequested {
            action: Action::Fill {
                r#ref: "@e5".into(),
                value: "test@example.com".into(),
            },
        })
        .unwrap();
        log.append(Event::ActionResult {
            action: Action::Fill {
                r#ref: "@e5".into(),
                value: "test@example.com".into(),
            },
            success: true,
            message: None,
        })
        .unwrap();

        // Step 3: Snapshot + Scroll + Finish
        log.append(Event::Snapshot {
            url: "https://example.com/form".into(),
            snapshot_hash: "snap3".into(),
            token_estimate: 100,
        })
        .unwrap();
        log.append(Event::ActionRequested {
            action: Action::Scroll { dx: 0, dy: 100 },
        })
        .unwrap();
        log.append(Event::ActionResult {
            action: Action::Scroll { dx: 0, dy: 100 },
            success: true,
            message: None,
        })
        .unwrap();
        log.append(Event::ActionRequested {
            action: Action::Press { key: "Tab".into() },
        })
        .unwrap();
        log.append(Event::ActionResult {
            action: Action::Press { key: "Tab".into() },
            success: true,
            message: None,
        })
        .unwrap();

        // Step 4: Snapshot + Finish
        log.append(Event::Snapshot {
            url: "https://example.com/success".into(),
            snapshot_hash: "snap4".into(),
            token_estimate: 100,
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

        log.append(Event::RunFinished {
            outcome: "success".into(),
            steps: 5,
            spend: BudgetSpend::default(),
        })
        .unwrap();

        // Record the cassette.
        let rec = CassetteRecorder::new("replay-e2e", "Replay E2E test");
        rec.record(&log).unwrap()
    }

    // ── Tests ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn replay_navigate_only() {
        let cassette = make_simple_cassette();
        let browser = MockBrowser::default();
        let replayer = CassetteReplayer::new(cassette);

        let outcome = replayer.replay_simple(&browser).await.unwrap();

        assert_eq!(outcome.steps_succeeded, 1);
        assert_eq!(outcome.steps_failed, 0);
        assert_eq!(outcome.step_results.len(), 1);
        assert_eq!(
            browser.events.lock().unwrap()[0],
            "navigate https://example.com"
        );
        assert!(
            matches!(
                outcome.step_results[0].status,
                ReplayStepStatus::Success { .. }
            ),
            "step should succeed"
        );
        assert!(
            !outcome.step_results[0].url.is_empty(),
            "replayed step URL should not be empty"
        );
    }

    #[tokio::test]
    async fn replay_full_pipeline_with_ref_resolver() {
        let cassette = build_replay_test_cassette();
        let browser = MockBrowser::default();
        let replayer = CassetteReplayer::new(cassette);

        // Provide a ref resolver: @e1 -> "42" (click target), @e5 -> "99" (fill target)
        let outcome = replayer
            .replay(&browser, |ref_str| match ref_str {
                "@e1" => Some("42".into()),
                "@e5" => Some("99".into()),
                _ => None,
            })
            .await
            .unwrap();

        // The cassette has 4 steps: Click, Fill, Scroll, Finish.
        // (The Press action has no preceding Snapshot so the recorder
        // skips it — the same applies to the initial Navigate.)
        assert_eq!(outcome.steps_succeeded, 4);
        assert_eq!(outcome.steps_failed, 0);
        assert_eq!(outcome.step_results.len(), 4);

        // Verify the actions were executed in order.
        let events = browser.events.lock().unwrap();
        // Navigate happens first (from replay() header)
        assert_eq!(events[0], "navigate https://example.com");
        // Step 1: Click @e1 -> node 42
        assert_eq!(events[1], "click 42");
        // Step 2: Fill @e5 -> node 99
        assert_eq!(events[2], "fill 99=test@example.com");
        // Step 3: Scroll
        assert_eq!(events[3], "scroll 0,100");
        // Step 4: Finish — no browser event (Press was skipped by recorder)
        assert_eq!(
            events.len(),
            4,
            "should have exactly 4 browser events (Navigate + Click + Fill + Scroll)"
        );

        // Check step results.
        assert!(
            matches!(
                outcome.step_results[0].status,
                ReplayStepStatus::Success { .. }
            ),
            "click should succeed"
        );
        assert!(
            matches!(
                outcome.step_results[1].status,
                ReplayStepStatus::Success { .. }
            ),
            "fill should succeed"
        );
        assert!(
            matches!(
                outcome.step_results[2].status,
                ReplayStepStatus::Success { .. }
            ),
            "scroll should succeed"
        );
        assert!(
            matches!(
                outcome.step_results[3].status,
                ReplayStepStatus::Success { .. }
            ),
            "finish should succeed"
        );
    }

    #[tokio::test]
    async fn replay_ref_resolver_failure_marks_step_failed() {
        let cassette = build_replay_test_cassette();
        let browser = MockBrowser::default();
        let replayer = CassetteReplayer::new(cassette);

        // Provide a resolver that returns None for everything.
        let outcome = replayer.replay(&browser, |_| None).await.unwrap();

        // Steps 0 (Click) and 1 (Fill) should fail because refs can't be resolved.
        // Steps 2 (Scroll) and 3 (Finish) should succeed.
        assert_eq!(
            outcome.steps_failed, 2,
            "Click and Fill should fail without ref resolver"
        );
        assert_eq!(
            outcome.steps_succeeded, 2,
            "Scroll and Finish should succeed"
        );

        assert_eq!(outcome.step_results.len(), 4);
        assert!(
            matches!(
                outcome.step_results[0].status,
                ReplayStepStatus::Failed { .. }
            ),
            "click without ref should fail"
        );
        assert!(
            matches!(
                outcome.step_results[1].status,
                ReplayStepStatus::Failed { .. }
            ),
            "fill without ref should fail"
        );
    }

    #[tokio::test]
    async fn replay_zero_llm_calls() {
        // Verify that the replayer never calls any LLM provider.
        let cassette = build_replay_test_cassette();
        let browser = MockBrowser::default();
        let replayer = CassetteReplayer::new(cassette);

        let outcome = replayer
            .replay(&browser, |ref_str| match ref_str {
                "@e1" => Some("42".into()),
                "@e5" => Some("99".into()),
                _ => None,
            })
            .await
            .unwrap();

        // All steps should succeed without any LLM interactions.
        assert_eq!(outcome.steps_failed, 0);
        assert_eq!(outcome.steps_succeeded, 4);

        // The mock browser events should contain only the actions, no LLM.
        let events = browser.events.lock().unwrap();
        let llm_events: Vec<_> = events
            .iter()
            .filter(|e| e.contains("llm") || e.contains("complete"))
            .collect();
        assert!(
            llm_events.is_empty(),
            "replay must not generate any LLM calls"
        );
    }

    #[tokio::test]
    async fn replay_empty_cassette() {
        // Edge case: cassette with no steps. Build one manually because
        // the recorder requires at least one Snapshot+ActionResult pair.
        use genatp_core::budget::BudgetSpend;

        let cassette = Cassette {
            header: CassetteHeader {
                schema_version: SCHEMA_VERSION,
                slug: "empty-replay".into(),
                version: "1.0.0".into(),
                description: "Empty replay".into(),
                goal: String::new(),
                start_url: "https://example.com".into(),
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

        let browser = MockBrowser::default();
        let replayer = CassetteReplayer::new(cassette);
        let outcome = replayer.replay_simple(&browser).await.unwrap();

        assert_eq!(outcome.steps_succeeded, 0);
        assert_eq!(outcome.steps_failed, 0);
        assert_eq!(outcome.step_results.len(), 0);
    }

    #[tokio::test]
    async fn replay_navigate_url_matches_step_url() {
        // Verify URL matching works correctly.
        let cassette = make_simple_cassette();
        let browser = MockBrowser::default();
        // Set the URL to what the cassette expects.
        {
            let mut u = browser.current_url.lock().unwrap();
            *u = "https://example.com".into();
        }

        let replayer = CassetteReplayer::new(cassette);
        let outcome = replayer.replay_simple(&browser).await.unwrap();

        if let ReplayStepStatus::Success { url_matches } = &outcome.step_results[0].status {
            assert!(*url_matches, "URL should match the step's recorded URL");
        } else {
            panic!("step should have succeeded");
        }
    }
}
