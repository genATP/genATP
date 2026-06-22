//! Event log reader — reads JSON-Lines event log files from disk and
//! reconstructs run summaries and step timelines for the debugger.
//!
//! Event log files are stored as `{run_dir}/{run_id}.jsonl` where each line
//! is a JSON-serialised [`EventEntry`]. The reader discovers runs by
//! scanning a directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use genatp_core::event_log::{Event, EventEntry};
use tracing::warn;

use crate::models::{RunSummary, StepDetail};

/// Default directory for event log files beneath the genATP data root.
pub const DEFAULT_RUNS_DIR: &str = "runs";

/// Discover and read all event log files in a directory.
///
/// Returns a map of run id → sorted list of event entries.
///
/// # Errors
/// Surfaces I/O errors when scanning the directory.
pub fn discover_runs(runs_dir: &Path) -> Result<BTreeMap<String, Vec<EventEntry>>, ReaderError> {
    let mut runs: BTreeMap<String, Vec<EventEntry>> = BTreeMap::new();

    if !runs_dir.exists() {
        return Ok(runs);
    }

    let read_dir = fs::read_dir(runs_dir).map_err(|e| ReaderError::Io {
        path: runs_dir.to_path_buf(),
        source: e,
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|e| ReaderError::Io {
            path: runs_dir.to_path_buf(),
            source: e,
        })?;
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "jsonl") {
            continue;
        }

        let run_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        match read_event_log(&path) {
            Ok(entries) => {
                runs.insert(run_id, entries);
            }
            Err(e) => {
                warn!(path = %path.display(), error = %e, "skipping corrupt event log");
            }
        }
    }

    Ok(runs)
}

/// Read a single JSON-Lines event log file, parsing each line as an
/// [`EventEntry`].
///
/// # Errors
/// Surfaces I/O and JSON parsing errors.
pub fn read_event_log(path: &Path) -> Result<Vec<EventEntry>, ReaderError> {
    let body = fs::read_to_string(path).map_err(|e| ReaderError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    let mut entries = Vec::new();
    for (line_idx, line) in body.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: EventEntry = serde_json::from_str(line).map_err(|e| ReaderError::Parse {
            path: path.to_path_buf(),
            line: line_idx + 1,
            source: e,
        })?;
        entries.push(entry);
    }

    entries.sort_by_key(|e| e.seq);
    Ok(entries)
}

/// Build a run summary from a list of event entries.
/// Goal is extracted from the `RunStarted` event by [`RunSummary::from_events`].
#[must_use]
pub fn summarize_run(id: &str, entries: &[EventEntry]) -> RunSummary {
    RunSummary::from_events(id.to_string(), entries)
}

/// Get a specific step (event entry) by sequence number.
#[must_use]
pub fn get_step(entries: &[EventEntry], seq: u64) -> Option<StepDetail> {
    entries
        .iter()
        .find(|e| e.seq == seq)
        .map(StepDetail::from_entry)
}

/// Get the step index closest to a given sequence number.
/// Returns `None` if the entries list is empty.
#[must_use]
pub fn find_nearest_step(entries: &[EventEntry], target_seq: u64) -> Option<&EventEntry> {
    if entries.is_empty() {
        return None;
    }
    // Binary search for the first entry with seq >= target_seq.
    let idx = entries.partition_point(|e| e.seq < target_seq);
    if idx < entries.len() {
        Some(&entries[idx])
    } else {
        entries.last()
    }
}

/// Find the snapshot hash for a given step sequence number.
/// Looks for the nearest Snapshot event at or before the target seq.
#[must_use]
pub fn find_snapshot_at_step(entries: &[EventEntry], target_seq: u64) -> Option<String> {
    // Walk backwards from target to find the nearest Snapshot event.
    let mut search_idx = entries.partition_point(|e| e.seq <= target_seq);
    if search_idx > 0 {
        search_idx = search_idx.saturating_sub(1);
    }
    for entry in entries[..=search_idx].iter().rev() {
        if let Event::Snapshot {
            ref snapshot_hash, ..
        } = entry.event
            && !snapshot_hash.is_empty()
        {
            return Some(snapshot_hash.clone());
        }
    }
    None
}

/// Run directory path within a base path (e.g. `~/.genatp/runs/`).
#[must_use]
pub fn runs_dir(base: &Path) -> PathBuf {
    base.join(DEFAULT_RUNS_DIR)
}

/// Event log file path for a given run id.
#[must_use]
pub fn event_log_path(runs_dir: &Path, run_id: &str) -> PathBuf {
    runs_dir.join(format!("{run_id}.jsonl"))
}

/// Errors from reading event logs.
#[derive(Debug, thiserror::Error)]
pub enum ReaderError {
    #[error("i/o error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parse error at {path}:{line}: {source}")]
    Parse {
        path: PathBuf,
        line: usize,
        #[source]
        source: serde_json::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn make_entries() -> Vec<EventEntry> {
        let mut entries = Vec::new();
        let mut prev = String::new();

        let e = make_entry(
            0,
            Event::RunStarted {
                task_id: "test-run".into(),
                goal: "Test goal".into(),
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

        entries
    }

    #[test]
    fn discovers_and_reads_event_log() {
        let tmp = tempfile::tempdir().unwrap();
        let runs = runs_dir(tmp.path());
        fs::create_dir_all(&runs).unwrap();

        let entries = make_entries();
        let log_path = event_log_path(&runs, "test-run");
        let mut file = fs::File::create(&log_path).unwrap();
        for entry in &entries {
            let line = serde_json::to_string(entry).unwrap();
            use std::io::Write;
            writeln!(file, "{line}").unwrap();
        }
        drop(file);

        let all_runs = discover_runs(&runs).unwrap();
        assert_eq!(all_runs.len(), 1);
        let loaded = all_runs.get("test-run").unwrap();
        assert_eq!(loaded.len(), 5);
    }

    #[test]
    fn read_event_log_parses_correctly() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("events.jsonl");
        let entries = make_entries();
        let mut file = fs::File::create(&path).unwrap();
        for entry in &entries {
            let line = serde_json::to_string(entry).unwrap();
            use std::io::Write;
            writeln!(file, "{line}").unwrap();
        }
        drop(file);

        let loaded = read_event_log(&path).unwrap();
        assert_eq!(loaded.len(), 5);
        assert_eq!(loaded[0].seq, 0);
        assert_eq!(loaded[4].seq, 4);
    }

    #[test]
    fn summarize_run_works() {
        let entries = make_entries();
        let summary = summarize_run("test-run", &entries);
        assert_eq!(summary.id, "test-run");
        assert_eq!(summary.goal, "Test goal");
        assert_eq!(summary.step_count, 1);
        assert_eq!(summary.outcome, "success");
        assert!(summary.hash_chain_valid);
    }

    #[test]
    fn find_snapshot_at_step_works() {
        let entries = make_entries();
        let hash = find_snapshot_at_step(&entries, 2);
        assert_eq!(hash, Some("abc123".to_string()));
    }

    #[test]
    fn get_step_returns_correct_step() {
        let entries = make_entries();
        let step = get_step(&entries, 1).unwrap();
        assert_eq!(step.event_type, "snapshot");
        assert_eq!(step.seq, 1);
    }

    #[test]
    fn detect_tampered_hash_chain() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("events.jsonl");

        // Write valid entries.
        let entries = make_entries();
        let mut file = fs::File::create(&path).unwrap();
        for entry in &entries {
            let line = serde_json::to_string(entry).unwrap();
            use std::io::Write;
            writeln!(file, "{line}").unwrap();
        }
        drop(file);

        // Tamper with the first entry on disk.
        let mut body = fs::read_to_string(&path).unwrap();
        body = body.replace("Test goal", "TAMPERED GOAL");
        fs::write(&path, body).unwrap();

        let loaded = read_event_log(&path).unwrap();
        let summary = summarize_run("test-run", &loaded);
        assert!(!summary.hash_chain_valid);
        assert_eq!(summary.first_bad_seq, Some(0));
    }
}
