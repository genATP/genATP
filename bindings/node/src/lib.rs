//! `genatp-node` — TypeScript/Node SDK bindings via NAPI-RS.
//!
//! Phase 10 deliverable (`v0.6.0`). Exposes the full genATP engine as a
//! native Node.js addon via `#[napi]` exports.
//!
//! ## Architecture
//!
//! - **Sync-first native API** — every exported function spawns a Tokio
//!   runtime internally, so Node callers never need to `await` native calls.
//!   The TypeScript wrapper provides idiomatic async wrappers.
//! - **`napi` classes** — `BrowserSession` is a native class so the JS
//!   garbage collector manages the browser lifetime correctly.
//! - **`serde-json` feature** — structured data round-trips via
//!   `serde_json::Value` instead of manual string serialisation.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use genatp_browser::launch::{HeadlessMode, LaunchOptions};
use genatp_core::budget::{BudgetLimits, Pricing};
use genatp_core::event_log::EventLog;
use genatp_core::orchestrator::{OrchestratorConfig, RunOutcome, run as run_task};
use genatp_core::page_adapter::PageAdapter;
use genatp_core::task::Task;
use genatp_llm::{MockProvider, ProviderKind, build_provider, resolve_from_env};
use genatp_mcp::{McpSession, SessionConfig, tools as mcp_tools};
use napi::bindgen_prelude::*;
use napi_derive::napi;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a Tokio multi-thread runtime. Called once per top-level function
/// invocation; the overhead is negligible compared to a browser launch.
fn build_runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::from_reason(format!("failed to build runtime: {e}")))
}

/// Convert a display-able error into a napi `Error`.
fn to_napi_err<E: std::fmt::Display>(e: E) -> Error {
    Error::from_reason(e.to_string())
}

// ---------------------------------------------------------------------------
// Exported types
// ---------------------------------------------------------------------------

/// Result of a single agent run.
#[napi(object)]
#[derive(Clone)]
pub struct NativeRunResult {
    /// `"success"`, `"aborted"`, or `"error"`.
    pub outcome: String,
    /// Final summary for success runs; abort reason or error message otherwise.
    pub summary: String,
    /// Number of orchestrator steps executed.
    pub steps: i32,
    /// Total prompt + completion tokens charged.
    pub tokens: i32,
    /// Cumulative USD spend (0 for free providers).
    pub usd: f64,
    /// Number of events recorded in the hash-chained event log.
    pub event_count: i32,
}

/// Snapshot of a resolved LLM provider.
#[napi(object)]
#[derive(Clone)]
pub struct NativeProviderInfo {
    /// Provider kind: `"anthropic"`, `"openai"`, `"nim"`, `"ollama"`, `"mock"`.
    pub kind: String,
    /// Provider's stable name.
    pub name: String,
    /// Default model id.
    pub model: String,
}

/// One MCP tool descriptor.
#[napi(object)]
#[derive(Clone)]
pub struct NativeToolDescriptor {
    /// Tool name (e.g. `"navigate"`, `"click"`).
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON-schema string for the tool's arguments.
    pub input_schema_json: String,
}

// ---------------------------------------------------------------------------
// BrowserSession — native class wrapping McpSession
// ---------------------------------------------------------------------------

/// A browser session that owns a Chrome subprocess. Must be closed explicitly.
#[napi]
pub struct JsBrowserSession {
    session: McpSession,
    runtime: Option<tokio::runtime::Runtime>,
}

#[napi]
impl JsBrowserSession {
    /// Create a new browser session. Chrome is not launched until the first
    /// tool call (lazy initialisation).
    #[napi(constructor)]
    pub fn new(headless: Option<bool>, user_data_dir: Option<String>) -> Result<Self> {
        let runtime = build_runtime()?;
        let cfg = SessionConfig {
            headless: headless.unwrap_or(true),
            user_data_dir: user_data_dir.map(std::path::PathBuf::from),
            pool_capacity: 1,
            ..SessionConfig::default()
        };
        let session = runtime
            .block_on(McpSession::new(cfg))
            .map_err(to_napi_err)?;
        Ok(Self {
            session,
            runtime: Some(runtime),
        })
    }

    /// Navigate to a URL. Returns the final URL after redirects.
    #[napi]
    pub fn navigate(&self, url: String) -> Result<String> {
        let rt = self
            .runtime
            .as_ref()
            .ok_or_else(|| Error::from_reason("session closed".to_string()))?;
        rt.block_on(self.session.navigate(&url))
            .map_err(to_napi_err)
    }

    /// Capture a fresh perception snapshot. Returns JSON string.
    #[napi]
    pub fn snapshot(&self) -> Result<String> {
        let rt = self
            .runtime
            .as_ref()
            .ok_or_else(|| Error::from_reason("session closed".to_string()))?;
        rt.block_on(self.session.snapshot()).map_err(to_napi_err)
    }

    /// Click an element identified by `@eN` ref.
    #[napi]
    pub fn click(&self, refstr: String) -> Result<()> {
        let rt = self
            .runtime
            .as_ref()
            .ok_or_else(|| Error::from_reason("session closed".to_string()))?;
        rt.block_on(self.session.click(&refstr))
            .map_err(to_napi_err)
    }

    /// Fill a text input identified by `@eN` ref.
    #[napi]
    pub fn fill(&self, refstr: String, value: String) -> Result<()> {
        let rt = self
            .runtime
            .as_ref()
            .ok_or_else(|| Error::from_reason("session closed".to_string()))?;
        rt.block_on(self.session.fill(&refstr, &value))
            .map_err(to_napi_err)
    }

    /// Scroll the viewport by relative pixels.
    #[napi]
    pub fn scroll(&self, dx: i32, dy: i32) -> Result<()> {
        let rt = self
            .runtime
            .as_ref()
            .ok_or_else(|| Error::from_reason("session closed".to_string()))?;
        rt.block_on(self.session.scroll(dx, dy))
            .map_err(to_napi_err)
    }

    /// Return the current page URL.
    #[napi]
    pub fn current_url(&self) -> Result<String> {
        let rt = self
            .runtime
            .as_ref()
            .ok_or_else(|| Error::from_reason("session closed".to_string()))?;
        rt.block_on(self.session.current_url()).map_err(to_napi_err)
    }

    /// Navigate back in history. Returns `true` if navigation happened.
    #[napi]
    pub fn go_back(&self) -> Result<bool> {
        let rt = self
            .runtime
            .as_ref()
            .ok_or_else(|| Error::from_reason("session closed".to_string()))?;
        rt.block_on(self.session.go_back()).map_err(to_napi_err)
    }

    /// Take a full-viewport PNG screenshot. Returns base64-encoded string.
    #[napi]
    pub fn screenshot(&self) -> Result<String> {
        let rt = self
            .runtime
            .as_ref()
            .ok_or_else(|| Error::from_reason("session closed".to_string()))?;
        let png_bytes = rt
            .block_on(self.session.screenshot_png())
            .map_err(to_napi_err)?;
        Ok(base64::engine::general_purpose::STANDARD.encode(&png_bytes))
    }

    /// Extract visible text from the latest snapshot.
    #[napi]
    pub fn extract(&self, query: Option<String>) -> Result<String> {
        let rt = self
            .runtime
            .as_ref()
            .ok_or_else(|| Error::from_reason("session closed".to_string()))?;
        rt.block_on(self.session.extract(query.as_deref()))
            .map_err(to_napi_err)
    }

    /// Save auth state (cookies + localStorage) for a domain to the vault.
    #[napi]
    pub fn save_auth(&self, domain: String) -> Result<NativeSaveAuthSummary> {
        let rt = self
            .runtime
            .as_ref()
            .ok_or_else(|| Error::from_reason("session closed".to_string()))?;
        let summary = rt
            .block_on(self.session.save_auth(&domain))
            .map_err(to_napi_err)?;
        Ok(NativeSaveAuthSummary {
            cookies: summary.cookies as i32,
            local_storage: summary.local_storage as i32,
            session_storage: summary.session_storage as i32,
            cookie_names: summary.cookie_names,
        })
    }

    /// Load auth state for a domain from the vault into the current page.
    #[napi]
    pub fn load_auth(&self, domain: String) -> Result<NativeLoadAuthSummary> {
        let rt = self
            .runtime
            .as_ref()
            .ok_or_else(|| Error::from_reason("session closed".to_string()))?;
        let summary = rt
            .block_on(self.session.load_auth(&domain))
            .map_err(to_napi_err)?;
        Ok(NativeLoadAuthSummary {
            cookies: summary.cookies as i32,
            local_storage: summary.local_storage as i32,
            session_storage: summary.session_storage as i32,
        })
    }

    /// List all domains stored in the auth vault.
    #[napi]
    pub fn list_auth(&self) -> Result<Vec<NativeAuthDomain>> {
        let rt = self
            .runtime
            .as_ref()
            .ok_or_else(|| Error::from_reason("session closed".to_string()))?;
        let domains = rt.block_on(self.session.list_auth()).map_err(to_napi_err)?;
        Ok(domains
            .into_iter()
            .map(|(domain, count)| NativeAuthDomain {
                domain,
                entry_count: count as i32,
            })
            .collect())
    }

    /// Close the session. Idempotent.
    #[napi]
    pub fn close(&mut self) {
        self.runtime = None;
    }
}

/// Summary of a `save_auth` operation.
#[napi(object)]
#[derive(Clone)]
pub struct NativeSaveAuthSummary {
    pub cookies: i32,
    pub local_storage: i32,
    pub session_storage: i32,
    pub cookie_names: Vec<String>,
}

/// Summary of a `load_auth` operation.
#[napi(object)]
#[derive(Clone)]
pub struct NativeLoadAuthSummary {
    pub cookies: i32,
    pub local_storage: i32,
    pub session_storage: i32,
}

/// One auth domain entry.
#[napi(object)]
#[derive(Clone)]
pub struct NativeAuthDomain {
    pub domain: String,
    pub entry_count: i32,
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Return the crate version.
#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Resolve an LLM provider from environment variables.
#[napi]
pub fn resolve_provider_from_env() -> Result<NativeProviderInfo> {
    let resolved = resolve_from_env().map_err(to_napi_err)?;
    Ok(NativeProviderInfo {
        kind: resolved.kind.as_str().to_owned(),
        name: resolved.provider.name().to_owned(),
        model: resolved.model,
    })
}

/// Return the canonical list of MCP tool descriptors as an array of objects.
#[napi]
pub fn list_mcp_tools() -> Result<Vec<NativeToolDescriptor>> {
    let mut out = Vec::new();
    for d in mcp_tools::descriptors() {
        out.push(NativeToolDescriptor {
            name: d.name,
            description: d.description,
            input_schema_json: serde_json::to_string(&d.input_schema).map_err(to_napi_err)?,
        });
    }
    Ok(out)
}

/// Build info — returns a JS object with crate versions and capabilities.
#[napi]
pub fn build_info() -> Result<serde_json::Value> {
    let tools: Vec<String> = mcp_tools::descriptors()
        .into_iter()
        .map(|d| d.name)
        .collect();
    Ok(serde_json::json!({
        "genatp_node": env!("CARGO_PKG_VERSION"),
        "genatp_core": genatp_core::version(),
        "genatp_llm": genatp_llm::version(),
        "genatp_browser": genatp_browser::version(),
        "genatp_perception": genatp_perception::version(),
        "genatp_mcp": genatp_mcp::version(),
        "mcp_tools": tools,
        "providers": ["anthropic", "openai", "nim", "ollama", "mock"],
    }))
}

/// Run a single agent task. This is the heart of the Node SDK.
///
/// Parameters:
/// - `goal`: plain-English task description
/// - `startUrl`: URL the agent starts on
/// - `model`: LLM model id (optional — uses provider default)
/// - `maxSteps`: step ceiling (default 30)
/// - `maxTokens`: hard token budget (0 = unlimited)
/// - `maxWallSec`: wall-clock budget in seconds (0 = unlimited)
/// - `maxUsd`: dollar budget (0 = unlimited)
/// - `systemPrompt`: override the default system prompt
/// - `headless`: Chrome headless mode (default true)
/// - `userDataDir`: persistent profile directory (optional)
/// - `provider`: `"anthropic"`, `"openai"`, `"nim"`, `"ollama"`, `"mock"`
/// - `mockReply`: mock provider response (only used with provider="mock")
#[napi]
#[allow(clippy::too_many_arguments)]
pub fn run_agent(
    goal: String,
    start_url: String,
    model: Option<String>,
    max_steps: Option<i32>,
    max_tokens: Option<i32>,
    max_wall_sec: Option<i64>,
    max_usd: Option<f64>,
    system_prompt: Option<String>,
    headless: Option<bool>,
    user_data_dir: Option<String>,
    provider: Option<String>,
    mock_reply: Option<String>,
) -> Result<NativeRunResult> {
    let runtime = build_runtime()?;
    let outcome = runtime.block_on(async move {
        // 1) Resolve provider.
        let (provider_arc, resolved_model): (Arc<dyn genatp_llm::LlmProvider>, String) =
            match provider.as_deref() {
                Some("mock") => {
                    let reply = mock_reply.unwrap_or_else(|| "done".to_owned());
                    let p = MockProvider::with_reply(reply);
                    (Arc::new(p), model.unwrap_or_else(|| "mock".to_owned()))
                }
                Some(other) => {
                    let kind = ProviderKind::parse(other).map_err(to_napi_err)?;
                    let resolved = build_provider(kind).map_err(to_napi_err)?;
                    let m = model.unwrap_or(resolved.model);
                    (resolved.provider, m)
                }
                None => {
                    let resolved = resolve_from_env().map_err(to_napi_err)?;
                    let m = model.unwrap_or(resolved.model);
                    (resolved.provider, m)
                }
            };

        // 2) Launch browser.
        let browser = genatp_browser::launch(LaunchOptions {
            headless: if headless.unwrap_or(true) {
                HeadlessMode::New
            } else {
                HeadlessMode::Visible
            },
            user_data_dir: user_data_dir.map(std::path::PathBuf::from),
            ..LaunchOptions::default()
        })
        .await
        .map_err(to_napi_err)?;
        let page = browser.new_page().await.map_err(to_napi_err)?;

        // 3) Build task.
        let id = format!("node-{}", now_ms());
        let task = Task {
            id,
            goal,
            start_url,
            model: resolved_model,
            system_prompt,
            max_steps: max_steps.unwrap_or(30) as u32,
            budget: BudgetLimits {
                max_tokens: max_tokens.unwrap_or(0) as u32,
                max_wall: Duration::from_secs(max_wall_sec.unwrap_or(0) as u64),
                max_usd: max_usd.unwrap_or(0.0),
                ..BudgetLimits::default()
            },
            pricing: Pricing::FREE,
            retry: genatp_core::RetryPolicy::default(),
        };

        // 4) Run orchestrator.
        let mut log = EventLog::in_memory();
        let result = run_task(OrchestratorConfig {
            task,
            browser: PageAdapter::new(page),
            provider: provider_arc,
            event_log: &mut log,
            snapshot_store: None,
        })
        .await;

        let event_count = log.entries().len() as i32;

        // 5) Extract final spend from event log.
        let last_spend = log.entries().iter().rev().find_map(|entry| {
            if let genatp_core::event_log::Event::RunFinished { spend, .. } = &entry.event {
                Some(*spend)
            } else {
                None
            }
        });
        let (tokens, usd) = last_spend
            .map(|s| (s.tokens as i32, s.usd))
            .unwrap_or((0, 0.0));

        let value = match result {
            Ok(RunOutcome::Success { summary, steps }) => NativeRunResult {
                outcome: "success".into(),
                summary,
                steps: steps as i32,
                tokens,
                usd,
                event_count,
            },
            Ok(RunOutcome::Aborted { reason, steps }) => NativeRunResult {
                outcome: "aborted".into(),
                summary: reason,
                steps: steps as i32,
                tokens,
                usd,
                event_count,
            },
            Err(e) => NativeRunResult {
                outcome: "error".into(),
                summary: e.to_string(),
                steps: 0,
                tokens,
                usd,
                event_count,
            },
        };

        // 6) Shutdown browser.
        let _ = browser
            .shutdown(Duration::from_secs(5))
            .await
            .map_err(|e| tracing::warn!(error = %e, "browser shutdown timed out"));

        Ok::<NativeRunResult, Error>(value)
    })?;
    Ok(outcome)
}

/// Milliseconds since Unix epoch — used for default task IDs.
fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}
