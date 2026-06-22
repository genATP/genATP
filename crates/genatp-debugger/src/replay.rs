//! Replay engine — reconstructs orchestrator state at a given step from the
//! event log and snapshot CAS store.
//!
//! The replay logic is straightforward:
//!
//! 1. Read the event log (JSON-Lines) for a run.
//! 2. Walk through events until reaching the target step.
//! 3. Collect the most recent snapshot hash and the last action result.
//! 4. Return a human-readable summary of the state at that point.

use std::path::Path;
use std::sync::{Arc, Mutex};

use genatp_perception::SnapshotStore;
use genatp_perception::snapshot::CompressedSnapshot;
use genatp_perception::store::ContentHash;

use crate::models::{RunSummary, SnapshotResponse, StepDetail};
use crate::reader;

/// Reconstructed state at a specific step in a run.
#[derive(Debug)]
pub struct ReplayState {
    /// Run summary (meta information).
    pub summary: RunSummary,
    /// The event entry at the target step, if found.
    pub current_step: Option<StepDetail>,
    /// Total entries in the log.
    pub total_entries: u64,
    /// Whether the target step is the last entry.
    pub is_at_end: bool,
}

/// Replay state with snapshot — combines [`ReplayState`] with the snapshot
/// at that point.
#[derive(Debug)]
pub struct ReplaySnapshot {
    pub state: ReplayState,
    pub snapshot: Option<SnapshotResponse>,
}

/// Replay a run from its event log, stopping at the given step (or the end).
///
/// If `step` is `None`, replays to the end of the log.
///
/// # Errors
/// Surfaces I/O and parse errors from reading the event log.
pub fn replay_to_step(
    runs_dir: &Path,
    run_id: &str,
    step: Option<u64>,
) -> Result<ReplayState, ReplayError> {
    let log_path = reader::event_log_path(runs_dir, run_id);
    let entries = reader::read_event_log(&log_path)?;

    if entries.is_empty() {
        return Err(ReplayError::EmptyLog {
            run_id: run_id.to_string(),
        });
    }

    let summary = reader::summarize_run(run_id, &entries);
    let target_seq = step.unwrap_or(u64::MAX);

    // Find the entry at or before target_seq.
    let current_entry = reader::find_nearest_step(&entries, target_seq);
    let current_step = current_entry.map(|e| StepDetail::from_entry(e));

    let total_entries = entries.len() as u64;
    let is_at_end = current_entry
        .map(|e| e.seq == entries.last().map(|l| l.seq).unwrap_or(0))
        .unwrap_or(true);

    Ok(ReplayState {
        summary,
        current_step,
        total_entries,
        is_at_end,
    })
}

/// Replay a run to a step and resolve the snapshot from the CAS store.
///
/// # Errors
/// Surfaces I/O, parse, and CAS store errors.
pub fn replay_to_step_with_snapshot(
    runs_dir: &Path,
    run_id: &str,
    step: Option<u64>,
    snapshot_store: Option<&Arc<Mutex<SnapshotStore>>>,
) -> Result<ReplaySnapshot, ReplayError> {
    let state = replay_to_step(runs_dir, run_id, step)?;
    let log_path = reader::event_log_path(runs_dir, run_id);
    let entries = reader::read_event_log(&log_path)?;

    let target_seq = step.unwrap_or(u64::MAX);
    let snapshot_hash = reader::find_snapshot_at_step(&entries, target_seq);

    let snapshot = match (snapshot_hash, snapshot_store) {
        (Some(hash), Some(store)) => {
            match ContentHash::from_hex(&hash) {
                Ok(content_hash) => {
                    let locked = store.lock().unwrap();
                    match locked.get(&content_hash) {
                        Ok(Some(snap)) => Some(SnapshotResponse {
                            snapshot: snap,
                            hash: hash.clone(),
                            step_seq: target_seq,
                        }),
                        Ok(None) => {
                            // Hash not in store — return a stub with just the hash.
                            Some(SnapshotResponse {
                                snapshot: CompressedSnapshot::empty(""),
                                hash,
                                step_seq: target_seq,
                            })
                        }
                        Err(e) => {
                            return Err(ReplayError::Store(format!("CAS get: {e}")));
                        }
                    }
                }
                Err(_) => {
                    // Not a valid content hash — might be a descriptive hash
                    // like "dom-fallback" or "vision-xxx".
                    Some(SnapshotResponse {
                        snapshot: CompressedSnapshot::empty(""),
                        hash,
                        step_seq: target_seq,
                    })
                }
            }
        }
        (Some(hash), None) => Some(SnapshotResponse {
            snapshot: CompressedSnapshot::empty(""),
            hash,
            step_seq: target_seq,
        }),
        (None, _) => None,
    };

    Ok(ReplaySnapshot { state, snapshot })
}

/// Errors from replay operations.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("event log: {0}")]
    Reader(#[from] crate::reader::ReaderError),
    #[error("empty event log for run `{run_id}`")]
    EmptyLog { run_id: String },
    #[error("store: {0}")]
    Store(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    use chrono::Utc;
    use genatp_core::action::Action;
    use genatp_core::budget::BudgetSpend;
    use genatp_core::event_log::{Event, EventEntry};

    fn make_entry(seq: u64, event: Event, prev_hash: &str) -> EventEntry {
        let payload = serde_json::to_vec(&event).unwrap();
        let mut hasher = blake3::Hasher::new();
        hasher.update(prev_hash.as_bytes());
        hasher.update(&payload);
        let hash = hex::encode(hasher.finalize().as_bytes());
        EventEntry {
            seq,
            at: Utc::now(),
            event,
            hash,
        }
    }

    fn write_test_log(dir: &Path, run_id: &str) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("{run_id}.jsonl"));

        let mut entries = Vec::new();
        let mut prev = String::new();

        let e = make_entry(
            0,
            Event::RunStarted {
                task_id: run_id.into(),
                goal: "Test replay".into(),
            },
            &prev,
        );
        prev = e.hash.clone();
        entries.push(e);

        let e = make_entry(
            1,
            Event::Snapshot {
                url: "https://example.com".into(),
                snapshot_hash: "abc123".into(),
                token_estimate: 400,
            },
            &prev,
        );
        prev = e.hash.clone();
        entries.push(e);

        let e = make_entry(
            2,
            Event::ActionRequested {
                action: Action::Click {
                    r#ref: "@e1".into(),
                },
            },
            &prev,
        );
        prev = e.hash.clone();
        entries.push(e);

        let e = make_entry(
            3,
            Event::ActionResult {
                action: Action::Click {
                    r#ref: "@e1".into(),
                },
                success: true,
                message: Some("clicked".into()),
            },
            &prev,
        );
        prev = e.hash.clone();
        entries.push(e);

        let e = make_entry(
            4,
            Event::RunFinished {
                outcome: "success".into(),
                steps: 1,
                spend: BudgetSpend::default(),
            },
            &prev,
        );
        entries.push(e);

        let mut file = fs::File::create(&path).unwrap();
        for entry in &entries {
            writeln!(file, "{}", serde_json::to_string(entry).unwrap()).unwrap();
        }
        path
    }

    #[test]
    fn replay_to_end() {
        let tmp = tempfile::tempdir().unwrap();
        let run_id = "test-run";
        write_test_log(tmp.path(), run_id);

        let result = replay_to_step(tmp.path(), run_id, None).unwrap();
        assert_eq!(result.summary.id, run_id);
        assert_eq!(result.total_entries, 5);
        assert!(result.is_at_end);
        assert_eq!(result.summary.outcome, "success");
    }

    #[test]
    fn replay_to_specific_step() {
        let tmp = tempfile::tempdir().unwrap();
        let run_id = "test-run";
        write_test_log(tmp.path(), run_id);

        let result = replay_to_step(tmp.path(), run_id, Some(2)).unwrap();
        assert!(result.current_step.is_some());
        assert_eq!(result.current_step.as_ref().unwrap().seq, 2);
        assert!(!result.is_at_end);
    }

    #[test]
    fn replay_to_step_beyond_log_returns_last() {
        let tmp = tempfile::tempdir().unwrap();
        let run_id = "test-run";
        write_test_log(tmp.path(), run_id);

        let result = replay_to_step(tmp.path(), run_id, Some(999)).unwrap();
        assert_eq!(result.total_entries, 5);
        assert!(result.is_at_end);
    }

    #[test]
    fn replay_empty_log_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("empty.jsonl");
        fs::write(&path, "").unwrap();

        let err = replay_to_step(tmp.path(), "empty", None).unwrap_err();
        assert!(matches!(err, ReplayError::EmptyLog { .. }));
    }

    #[test]
    fn replay_nonexistent_run_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let err = replay_to_step(tmp.path(), "no-such-run", None).unwrap_err();
        assert!(matches!(err, ReplayError::Reader(_)));
    }
}
