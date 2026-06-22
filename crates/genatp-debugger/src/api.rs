//! REST API for the time-travel debugger.
//!
//! Serves event log data and snapshot diffs to the web UI.
//! Endpoints:
//!
//! - `GET  /runs`                       — list all runs
//! - `GET  /runs/:id`                   — run summary
//! - `GET  /runs/:id/timeline`          — timeline events for the web UI
//! - `GET  /runs/:id/steps/:seq`        — specific step detail
//! - `GET  /runs/:id/steps/:seq/snapshot` — snapshot at a given step
//! - `GET  /runs/:id/diff?from=<N>&to=<M>` — diff between two steps
//! - `GET  /health`                     — health check

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use serde::Deserialize;
use tower_http::cors::CorsLayer;

use genatp_core::event_log::{Event, EventEntry};
use genatp_perception::store::ContentHash;
use genatp_perception::{CompressedSnapshot, SnapshotStore};

use crate::models::{DiffResponse, RunSummary, SnapshotResponse, TimelineEvent};
use crate::reader::{
    self, discover_runs, find_snapshot_at_step, get_step, read_event_log, summarize_run,
};

/// Shared application state for the debugger API.
/// `SnapshotStore` is wrapped in `Arc<Mutex<>>` because `rusqlite::Connection`
/// is not `Sync`.
#[derive(Clone)]
pub struct AppState {
    /// Directory where run event logs are stored (`{base}/runs/`).
    pub runs_dir: PathBuf,
    /// Optional snapshot store for resolving snapshot hashes into full snapshots.
    pub snapshot_store: Option<Arc<Mutex<SnapshotStore>>>,
}

/// Query parameters for the diff endpoint.
#[derive(Debug, Deserialize)]
pub struct DiffQuery {
    pub from: u64,
    pub to: u64,
}

/// Build the debugger API router.
pub fn router(state: AppState) -> Router {
    let cors = CorsLayer::permissive();
    Router::new()
        .route("/health", get(health_check))
        .route("/runs", get(list_runs))
        .route("/runs/{id}", get(get_run))
        .route("/runs/{id}/timeline", get(get_timeline))
        .route("/runs/{id}/steps/{seq}", get(get_step_handler))
        .route("/runs/{id}/steps/{seq}/snapshot", get(get_snapshot))
        .route("/runs/{id}/diff", get(get_diff))
        .layer(cors)
        .with_state(state)
}

/// Health check — returns 200 OK.
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// `GET /runs` — list all discovered runs.
async fn list_runs(State(state): State<AppState>) -> impl IntoResponse {
    let runs = match discover_runs(&state.runs_dir) {
        Ok(runs) => runs,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let summaries: Vec<RunSummary> = runs
        .into_iter()
        .map(|(id, entries)| summarize_run(&id, &entries))
        .collect();

    (StatusCode::OK, Json(serde_json::json!(summaries)))
}

/// `GET /runs/:id` — get a single run summary.
async fn get_run(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    let path = reader::event_log_path(&state.runs_dir, &id);
    let entries = match read_event_log(&path) {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": format!("run `{id}` not found: {e}") })),
            );
        }
    };

    let summary = summarize_run(&id, &entries);
    (StatusCode::OK, Json(serde_json::json!(summary)))
}

/// `GET /runs/:id/timeline` — timeline events for the web UI.
async fn get_timeline(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    let path = reader::event_log_path(&state.runs_dir, &id);
    let entries = match read_event_log(&path) {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": format!("run `{id}` not found: {e}") })),
            );
        }
    };

    let timeline: Vec<TimelineEvent> = entries.iter().filter_map(build_timeline_event).collect();

    (StatusCode::OK, Json(serde_json::json!(timeline)))
}

/// `GET /runs/:id/steps/:seq` — get a specific step.
async fn get_step_handler(
    State(state): State<AppState>,
    Path((id, seq_str)): Path<(String, String)>,
) -> impl IntoResponse {
    let seq: u64 = match seq_str.parse() {
        Ok(n) => n,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("invalid sequence number: {seq_str}") })),
            );
        }
    };

    let path = reader::event_log_path(&state.runs_dir, &id);
    let entries = match read_event_log(&path) {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": format!("run `{id}` not found: {e}") })),
            );
        }
    };

    match get_step(&entries, seq) {
        Some(step) => (StatusCode::OK, Json(serde_json::json!(step))),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("step {seq} not found in run `{id}`") })),
        ),
    }
}

/// `GET /runs/:id/steps/:seq/snapshot` — get the snapshot at a given step.
async fn get_snapshot(
    State(state): State<AppState>,
    Path((id, seq_str)): Path<(String, String)>,
) -> impl IntoResponse {
    let seq: u64 = match seq_str.parse() {
        Ok(n) => n,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("invalid sequence number: {seq_str}") })),
            );
        }
    };

    let path = reader::event_log_path(&state.runs_dir, &id);
    let entries = match read_event_log(&path) {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": format!("run `{id}` not found: {e}") })),
            );
        }
    };

    let snapshot_hash = match find_snapshot_at_step(&entries, seq) {
        Some(h) => h,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(
                    serde_json::json!({ "error": format!("no snapshot found at or before step {seq}") }),
                ),
            );
        }
    };

    // Attempt to resolve from CAS store.
    let store = match &state.snapshot_store {
        Some(s) => s,
        None => {
            return (
                StatusCode::OK,
                Json(serde_json::json!(SnapshotResponse {
                    snapshot: CompressedSnapshot::empty(""),
                    hash: snapshot_hash,
                    step_seq: seq,
                })),
            );
        }
    };

    let content_hash = match ContentHash::from_hex(&snapshot_hash) {
        Ok(h) => h,
        Err(_) => {
            // Not a valid hex hash (e.g., "dom-fallback" or "vision-xxx").
            return (
                StatusCode::OK,
                Json(serde_json::json!(SnapshotResponse {
                    snapshot: CompressedSnapshot::empty(""),
                    hash: snapshot_hash,
                    step_seq: seq,
                })),
            );
        }
    };

    let locked = store.lock().unwrap();
    match locked.get(&content_hash) {
        Ok(Some(snapshot)) => (
            StatusCode::OK,
            Json(serde_json::json!(SnapshotResponse {
                snapshot,
                hash: snapshot_hash,
                step_seq: seq,
            })),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "snapshot not found in CAS store" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("CAS store error: {e}") })),
        ),
    }
}

/// `GET /runs/:id/diff?from=<N>&to=<M>` — diff between two steps.
async fn get_diff(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DiffQuery>,
) -> impl IntoResponse {
    let path = reader::event_log_path(&state.runs_dir, &id);
    let entries = match read_event_log(&path) {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": format!("run `{id}` not found: {e}") })),
            );
        }
    };

    let store = match &state.snapshot_store {
        Some(s) => s,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "no snapshot store configured" })),
            );
        }
    };

    let from_hash = match find_snapshot_at_step(&entries, query.from) {
        Some(h) => h,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(
                    serde_json::json!({ "error": format!("no snapshot at or before step {}", query.from) }),
                ),
            );
        }
    };

    let to_hash = match find_snapshot_at_step(&entries, query.to) {
        Some(h) => h,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(
                    serde_json::json!({ "error": format!("no snapshot at or before step {}", query.to) }),
                ),
            );
        }
    };

    if from_hash == to_hash {
        // Same hash — return empty diff.
        return (
            StatusCode::OK,
            Json(serde_json::json!(DiffResponse {
                from_seq: query.from,
                to_seq: query.to,
                from_url: String::new(),
                to_url: String::new(),
                op_count: 0,
                ops: vec![],
            })),
        );
    }

    // Check if both are valid hex content hashes.
    let from_content = ContentHash::from_hex(&from_hash);
    let to_content = ContentHash::from_hex(&to_hash);
    let (from_content, to_content) = match (from_content, to_content) {
        (Ok(f), Ok(t)) => (f, t),
        _ => {
            // Not content-addressed hashes (e.g., "dom-fallback").
            return (
                StatusCode::OK,
                Json(serde_json::json!(DiffResponse {
                    from_seq: query.from,
                    to_seq: query.to,
                    from_url: String::new(),
                    to_url: String::new(),
                    op_count: 0,
                    ops: vec![],
                })),
            );
        }
    };

    let locked = store.lock().unwrap();
    let prev = match locked.get(&from_content) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "from-snapshot not found in CAS store" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("CAS store error: {e}") })),
            );
        }
    };

    let curr = match locked.get(&to_content) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "to-snapshot not found in CAS store" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("CAS store error: {e}") })),
            );
        }
    };

    let diff = DiffResponse::from_snapshots(query.from, query.to, &prev, &curr);
    (StatusCode::OK, Json(serde_json::json!(diff)))
}

/// Start the debugger API server on the given address.
///
/// # Errors
/// Surfaces binding errors from axum.
pub async fn serve(state: AppState, addr: &std::net::SocketAddr) -> Result<(), anyhow::Error> {
    let app = router(state);
    tracing::info!(%addr, "starting debugger API server");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Build a [`TimelineEvent`] from an event entry for the web UI.
fn build_timeline_event(entry: &EventEntry) -> Option<TimelineEvent> {
    match &entry.event {
        Event::RunStarted { goal, .. } => Some(TimelineEvent::RunStart {
            seq: entry.seq,
            at: entry.at,
            goal: goal.clone(),
        }),
        Event::Snapshot {
            url,
            snapshot_hash,
            token_estimate,
        } => Some(TimelineEvent::Snapshot {
            seq: entry.seq,
            at: entry.at,
            url: url.clone(),
            snapshot_hash: snapshot_hash.clone(),
            token_estimate: *token_estimate,
        }),
        Event::ActionRequested { action } => Some(TimelineEvent::Action {
            seq: entry.seq,
            at: entry.at,
            action_type: action.tool_name().to_string(),
            summary: format!("{action:?}"),
            success: None,
        }),
        Event::ActionResult {
            action, success, ..
        } => Some(TimelineEvent::Action {
            seq: entry.seq,
            at: entry.at,
            action_type: action.tool_name().to_string(),
            summary: format!("{action:?}"),
            success: Some(*success),
        }),
        Event::LlmRequest {
            provider, model, ..
        } => Some(TimelineEvent::LlmCall {
            seq: entry.seq,
            at: entry.at,
            provider: provider.clone(),
            model: model.clone(),
            tokens: None,
            finish_reason: None,
        }),
        Event::LlmResponse {
            usage,
            finish_reason,
            ..
        } => Some(TimelineEvent::LlmCall {
            seq: entry.seq,
            at: entry.at,
            provider: String::new(),
            model: String::new(),
            tokens: Some(usage.total_tokens),
            finish_reason: Some(finish_reason.clone()),
        }),
        Event::BudgetWarn { dimension, .. } => Some(TimelineEvent::BudgetEvent {
            seq: entry.seq,
            at: entry.at,
            event_kind: "warn".into(),
            dimension: dimension.clone(),
        }),
        Event::BudgetExceeded { dimension, .. } => Some(TimelineEvent::BudgetEvent {
            seq: entry.seq,
            at: entry.at,
            event_kind: "exceeded".into(),
            dimension: dimension.clone(),
        }),
        Event::RunFinished { outcome, steps, .. } => Some(TimelineEvent::RunFinish {
            seq: entry.seq,
            at: entry.at,
            outcome: outcome.clone(),
            steps: *steps,
        }),
        // Skip LlmError and PolicyDecision for the condensed timeline.
        Event::LlmError { .. } | Event::PolicyDecision { .. } => None,
    }
}
