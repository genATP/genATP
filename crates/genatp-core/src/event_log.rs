//! Append-only event log for orchestrator runs.
//!
//! The Phase 13 debugger replays runs from this log. Two invariants make
//! that possible:
//!
//! 1. **Append-only.** A new entry never overwrites or reorders existing
//!    entries. [`EventLog`] holds an in-memory `Vec<EventEntry>` and
//!    flushes serialized JSON-lines to disk if a path is configured.
//!
//! 2. **Hash-chained.** Entry `n`'s `hash` field is
//!    `BLAKE3(prev_hash || canonical_json_payload)`. Any tampering
//!    invalidates every subsequent entry — [`EventLog::verify`] walks the
//!    chain and surfaces the first mismatch.
//!
//! Events are deliberately small and provider-neutral. The orchestrator
//! is the only writer; tests verify the chain via [`EventLog::verify`].

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::budget::BudgetSpend;
use genatp_llm::Usage;

/// Concrete event types the orchestrator can emit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// Run started.
    RunStarted { task_id: String, goal: String },
    /// New snapshot captured by perception.
    Snapshot {
        url: String,
        snapshot_hash: String,
        token_estimate: usize,
    },
    /// LLM call about to be made.
    LlmRequest {
        provider: String,
        model: String,
        prompt_messages: u32,
    },
    /// LLM responded successfully.
    LlmResponse {
        provider: String,
        model: String,
        usage: Usage,
        finish_reason: String,
    },
    /// LLM call failed (after exhausting retries).
    LlmError {
        provider: String,
        message: String,
        retryable: bool,
    },
    /// Action about to dispatch.
    ActionRequested { action: Action },
    /// Action executed.
    ActionResult {
        action: Action,
        success: bool,
        message: Option<String>,
    },
    /// Budget warning fired (soft).
    BudgetWarn {
        dimension: String,
        spend: BudgetSpend,
    },
    /// Budget hard limit hit.
    BudgetExceeded {
        dimension: String,
        spend: BudgetSpend,
    },
    /// Perception policy decision (Phase 12).
    PolicyDecision {
        mode: String,
        reason: String,
        named_element_count: usize,
        stripped_node_count: usize,
    },
    /// Run finished — `outcome` is `"success"`, `"failure"`, or `"aborted"`.
    RunFinished {
        outcome: String,
        steps: u32,
        spend: BudgetSpend,
    },
}

/// Single entry in the chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEntry {
    /// 0-based monotonically increasing index.
    pub seq: u64,
    /// UTC timestamp of emission.
    pub at: DateTime<Utc>,
    /// Payload.
    pub event: Event,
    /// `hex(blake3(prev_hash || canonical_json(event)))`. `prev_hash` is
    /// `""` for the first entry.
    pub hash: String,
}

/// Append-only event log.
#[derive(Debug)]
pub struct EventLog {
    entries: Vec<EventEntry>,
    path: Option<PathBuf>,
}

impl EventLog {
    /// In-memory only.
    #[must_use]
    pub fn in_memory() -> Self {
        Self {
            entries: Vec::new(),
            path: None,
        }
    }

    /// Persisted to `path` as JSON-Lines.
    ///
    /// # Errors
    /// Surfaces I/O failures creating the file's parent directory.
    pub fn at_path(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)?;
        }
        Ok(Self {
            entries: Vec::new(),
            path: Some(path),
        })
    }

    /// Number of entries written.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Borrow entries (read-only).
    #[must_use]
    pub fn entries(&self) -> &[EventEntry] {
        &self.entries
    }

    /// Append an event.
    ///
    /// # Errors
    /// Surfaces JSON serialisation failures (rare; payloads are owned) and
    /// disk I/O when persisting.
    pub fn append(&mut self, event: Event) -> Result<&EventEntry, EventLogError> {
        let prev_hash = self.entries.last().map(|e| e.hash.as_str()).unwrap_or("");
        let payload = serde_json::to_vec(&event)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(prev_hash.as_bytes());
        hasher.update(&payload);
        let hash = hex::encode(hasher.finalize().as_bytes());

        let seq = self.entries.len() as u64;
        let entry = EventEntry {
            seq,
            at: Utc::now(),
            event,
            hash,
        };

        if let Some(path) = &self.path {
            let line = serde_json::to_string(&entry)?;
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(EventLogError::Io)?;
            file.write_all(line.as_bytes()).map_err(EventLogError::Io)?;
            file.write_all(b"\n").map_err(EventLogError::Io)?;
        }

        self.entries.push(entry);
        Ok(self.entries.last().expect("we just pushed"))
    }

    /// Walk the chain and recompute every hash. Returns the index of the
    /// first bad entry (None if all good).
    ///
    /// # Errors
    /// Surfaces serialisation failures (extremely unlikely; payloads are
    /// already in memory).
    pub fn verify(&self) -> Result<Option<u64>, EventLogError> {
        let mut prev_hash = String::new();
        for (idx, entry) in self.entries.iter().enumerate() {
            let payload = serde_json::to_vec(&entry.event)?;
            let mut hasher = blake3::Hasher::new();
            hasher.update(prev_hash.as_bytes());
            hasher.update(&payload);
            let expected = hex::encode(hasher.finalize().as_bytes());
            if expected != entry.hash {
                return Ok(Some(idx as u64));
            }
            prev_hash = entry.hash.clone();
        }
        Ok(None)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EventLogError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}

impl From<EventLogError> for crate::error::CoreError {
    fn from(value: EventLogError) -> Self {
        match value {
            EventLogError::Io(e) => crate::error::CoreError::Io(e),
            EventLogError::Serde(e) => crate::error::CoreError::Serde(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_increments_seq_and_chains_hash() {
        let mut log = EventLog::in_memory();
        log.append(Event::RunStarted {
            task_id: "t1".into(),
            goal: "test".into(),
        })
        .unwrap();
        log.append(Event::Snapshot {
            url: "https://example.com".into(),
            snapshot_hash: "abc".into(),
            token_estimate: 100,
        })
        .unwrap();
        assert_eq!(log.entries()[0].seq, 0);
        assert_eq!(log.entries()[1].seq, 1);
        // Hashes are distinct and non-empty.
        assert_ne!(log.entries()[0].hash, log.entries()[1].hash);
        assert!(!log.entries()[0].hash.is_empty());
    }

    #[test]
    fn verify_detects_tampering() {
        let mut log = EventLog::in_memory();
        log.append(Event::RunStarted {
            task_id: "t1".into(),
            goal: "test".into(),
        })
        .unwrap();
        log.append(Event::Snapshot {
            url: "https://example.com".into(),
            snapshot_hash: "abc".into(),
            token_estimate: 100,
        })
        .unwrap();

        assert_eq!(log.verify().unwrap(), None);
        // Tamper with the first entry's payload.
        log.entries[0].event = Event::RunStarted {
            task_id: "t1".into(),
            goal: "TAMPERED".into(),
        };
        assert_eq!(log.verify().unwrap(), Some(0));
    }

    #[test]
    fn persists_to_disk_as_json_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("events.jsonl");
        let mut log = EventLog::at_path(&path).unwrap();
        for i in 0..3 {
            log.append(Event::Snapshot {
                url: format!("http://x/{i}"),
                snapshot_hash: format!("{i}"),
                token_estimate: i as usize * 10,
            })
            .unwrap();
        }
        let body = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = body.lines().collect();
        assert_eq!(lines.len(), 3);
        // Each line round-trips.
        for line in lines {
            let entry: EventEntry = serde_json::from_str(line).unwrap();
            assert!(!entry.hash.is_empty());
        }
    }
}
