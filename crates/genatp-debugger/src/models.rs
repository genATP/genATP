//! API response models for the time-travel debugger.
//!
//! These types are serialised to JSON for the REST API (axum handlers)
//! and consumed by both the CLI and the React web UI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use genatp_core::event_log::{Event, EventEntry};
use genatp_perception::snapshot::CompressedSnapshot;

/// A run reconstructed from an event log JSON-Lines file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    /// Unique run identifier (derived from the event log filename stem).
    pub id: String,
    /// Task goal from the `RunStarted` event.
    pub goal: String,
    /// Total event entries in the log.
    pub entry_count: u64,
    /// Number of agent steps (actions dispatched).
    pub step_count: u32,
    /// Timestamp of the first event.
    pub started_at: Option<DateTime<Utc>>,
    /// Timestamp of the last event.
    pub finished_at: Option<DateTime<Utc>>,
    /// Outcome: `"success"`, `"failure"`, `"aborted"`, or `"unknown"`.
    pub outcome: String,
    /// Whether the hash chain verifies completely.
    pub hash_chain_valid: bool,
    /// Index of the first hash-chain violation, if any.
    pub first_bad_seq: Option<u64>,
}

/// A single event with additional computed metadata for the debugger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDetail {
    /// Monotonically increasing sequence number.
    pub seq: u64,
    /// UTC timestamp.
    pub at: DateTime<Utc>,
    /// Event type discriminator (e.g. `"snapshot"`, `"action_requested"`).
    pub event_type: String,
    /// Human-readable summary of the event payload.
    pub summary: String,
    /// Raw event JSON (for the full detail view).
    pub raw_event: serde_json::Value,
    /// Snapshot hash if this is a `Snapshot` event.
    pub snapshot_hash: Option<String>,
    /// Action description if this is an `ActionRequested` / `ActionResult` event.
    pub action_description: Option<String>,
}

/// Response wrapping a compressed snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotResponse {
    pub snapshot: CompressedSnapshot,
    pub hash: String,
    pub step_seq: u64,
}

/// Diff between two steps (patch operations).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResponse {
    pub from_seq: u64,
    pub to_seq: u64,
    pub from_url: String,
    pub to_url: String,
    pub op_count: usize,
    pub ops: Vec<DiffOpView>,
}

/// A human-readable diff operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffOpView {
    pub op: String,
    pub r#ref: String,
    pub role: Option<String>,
    pub name: Option<String>,
}

/// Aggregated event types for timeline rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TimelineEvent {
    RunStart {
        seq: u64,
        at: DateTime<Utc>,
        goal: String,
    },
    Snapshot {
        seq: u64,
        at: DateTime<Utc>,
        url: String,
        snapshot_hash: String,
        token_estimate: usize,
    },
    Action {
        seq: u64,
        at: DateTime<Utc>,
        action_type: String,
        summary: String,
        success: Option<bool>,
    },
    LlmCall {
        seq: u64,
        at: DateTime<Utc>,
        provider: String,
        model: String,
        tokens: Option<u32>,
        finish_reason: Option<String>,
    },
    BudgetEvent {
        seq: u64,
        at: DateTime<Utc>,
        event_kind: String,
        dimension: String,
    },
    RunFinish {
        seq: u64,
        at: DateTime<Utc>,
        outcome: String,
        steps: u32,
    },
}

impl RunSummary {
    /// Build a summary from a list of event entries.
    /// Extracts the goal from the `RunStarted` event if present.
    #[must_use]
    pub fn from_events(id: String, entries: &[EventEntry]) -> Self {
        let started_at = entries.first().map(|e| e.at);
        let finished_at = entries.last().map(|e| e.at);

        let mut goal = String::new();
        let mut step_count: u32 = 0;
        let mut outcome = "unknown".to_string();

        for entry in entries {
            match &entry.event {
                Event::RunStarted { goal: g, .. } => {
                    goal.clone_from(g);
                }
                Event::ActionResult { .. } => {
                    step_count = step_count.saturating_add(1);
                }
                Event::RunFinished { outcome: o, .. } => {
                    outcome = o.clone();
                }
                _ => {}
            }
        }

        // Verify hash chain.
        let mut hash_chain_valid = true;
        let mut first_bad_seq: Option<u64> = None;
        let mut prev_hash = String::new();
        for entry in entries {
            let payload = match serde_json::to_vec(&entry.event) {
                Ok(p) => p,
                Err(_) => {
                    hash_chain_valid = false;
                    first_bad_seq = Some(entry.seq);
                    break;
                }
            };
            let mut hasher = blake3::Hasher::new();
            hasher.update(prev_hash.as_bytes());
            hasher.update(&payload);
            let expected = hex::encode(hasher.finalize().as_bytes());
            if expected != entry.hash {
                hash_chain_valid = false;
                first_bad_seq = Some(entry.seq);
                break;
            }
            prev_hash = entry.hash.clone();
        }

        Self {
            id,
            goal,
            entry_count: entries.len() as u64,
            step_count,
            started_at,
            finished_at,
            outcome,
            hash_chain_valid,
            first_bad_seq,
        }
    }
}

impl StepDetail {
    /// Build a step detail from a single event entry.
    #[must_use]
    pub fn from_entry(entry: &EventEntry) -> Self {
        let raw_event = serde_json::to_value(&entry.event).unwrap_or_default();
        let event_type = match &entry.event {
            Event::RunStarted { .. } => "run_started",
            Event::Snapshot { .. } => "snapshot",
            Event::LlmRequest { .. } => "llm_request",
            Event::LlmResponse { .. } => "llm_response",
            Event::LlmError { .. } => "llm_error",
            Event::ActionRequested { .. } => "action_requested",
            Event::ActionResult { .. } => "action_result",
            Event::BudgetWarn { .. } => "budget_warn",
            Event::BudgetExceeded { .. } => "budget_exceeded",
            Event::PolicyDecision { .. } => "policy_decision",
            Event::RunFinished { .. } => "run_finished",
        }
        .to_string();

        let (summary, snapshot_hash, action_description) = match &entry.event {
            Event::RunStarted { task_id, goal } => {
                (format!("Run started: {goal} (task {task_id})"), None, None)
            }
            Event::Snapshot {
                url,
                snapshot_hash,
                token_estimate,
            } => (
                format!("Snapshot @ {url} (~{token_estimate} tokens)"),
                Some(snapshot_hash.clone()),
                None,
            ),
            Event::LlmRequest {
                provider,
                model,
                prompt_messages,
            } => (
                format!("LLM request: {provider}/{model} ({prompt_messages} messages)"),
                None,
                None,
            ),
            Event::LlmResponse {
                provider,
                model,
                usage,
                finish_reason,
            } => (
                format!(
                    "LLM response: {provider}/{model} — {} tokens total, reason: {finish_reason}",
                    usage.total_tokens
                ),
                None,
                None,
            ),
            Event::LlmError {
                provider,
                message,
                retryable,
            } => (
                format!("LLM error: {provider} — {message} (retryable: {retryable})"),
                None,
                None,
            ),
            Event::ActionRequested { action } => {
                let desc = format!("Action requested: {action:?}");
                (desc.clone(), None, Some(desc))
            }
            Event::ActionResult {
                action,
                success,
                message,
            } => {
                let desc = format!(
                    "Action result: {action:?} — {status}{msg}",
                    status = if *success { "ok" } else { "FAIL" },
                    msg = message
                        .as_ref()
                        .map(|m| format!(" ({m})"))
                        .unwrap_or_default()
                );
                (desc.clone(), None, Some(desc))
            }
            Event::BudgetWarn { dimension, spend } => (
                format!("Budget warning: {dimension} at {} tokens", spend.tokens),
                None,
                None,
            ),
            Event::BudgetExceeded { dimension, spend } => (
                format!("Budget exceeded: {dimension} at {} tokens", spend.tokens),
                None,
                None,
            ),
            Event::PolicyDecision { mode, reason, .. } => (
                format!("Policy decision: mode={mode}, reason={reason}"),
                None,
                None,
            ),
            Event::RunFinished { outcome, steps, .. } => (
                format!("Run finished: {outcome} after {steps} steps"),
                None,
                None,
            ),
        };

        Self {
            seq: entry.seq,
            at: entry.at,
            event_type,
            summary,
            raw_event,
            snapshot_hash,
            action_description,
        }
    }
}

impl DiffResponse {
    /// Build a diff response from two snapshots.
    #[must_use]
    pub fn from_snapshots(
        from_seq: u64,
        to_seq: u64,
        prev: &CompressedSnapshot,
        curr: &CompressedSnapshot,
    ) -> Self {
        let patch = genatp_perception::diff(prev, curr);
        let ops: Vec<DiffOpView> = patch
            .ops
            .iter()
            .map(|op| {
                let (op_str, r#ref, role, name) = match op {
                    genatp_perception::Op::Remove { r#ref } => {
                        ("remove".into(), r#ref.clone(), None, None)
                    }
                    genatp_perception::Op::Add { node } => (
                        "add".into(),
                        node.r#ref.clone(),
                        Some(node.role.clone()),
                        node.name.clone(),
                    ),
                    genatp_perception::Op::Replace { node } => (
                        "replace".into(),
                        node.r#ref.clone(),
                        Some(node.role.clone()),
                        node.name.clone(),
                    ),
                };
                DiffOpView {
                    op: op_str,
                    r#ref,
                    role,
                    name,
                }
            })
            .collect();

        Self {
            from_seq,
            to_seq,
            from_url: patch.from_url,
            to_url: patch.to_url,
            op_count: patch.ops.len(),
            ops,
        }
    }
}
