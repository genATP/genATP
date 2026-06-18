//! `genatp-python` — PyO3 + maturin bindings for the Python SDK.
//!
//! Phase 5 deliverable. Exposes a single native module `genatp._native`
//! whose Python-facing helpers wrap the Rust orchestrator, the MCP
//! browser session, and the LLM provider factory. All heavy lifting
//! stays in Rust crates; the pure-Python layer in `python/genatp/`
//! provides the ergonomic `Agent` facade, LangChain / LlamaIndex
//! adapters, and async wrappers.
//!
//! Design choices that the rest of the SDK depends on:
//!
//! - **Sync-first API.** The native functions block; an `asyncio`-
//!   friendly wrapper at the Python level shells them out to a thread.
//!   This sidesteps the GIL-vs-`tokio` reactor dance and avoids pulling
//!   in `pyo3-async-runtimes` for v0.2.
//! - **Internal Tokio runtime.** We build a dedicated multi-thread
//!   runtime per native call so Python callers don't have to. The
//!   runtime is cheap relative to a real browser launch (the only
//!   thing in the call path that costs > 1 ms).
//! - **`extension-module` always on.** Reduces `cargo build` to a no-op
//!   for the linker — no libpython required on the build host.

#![forbid(unsafe_code)]
#![allow(
    clippy::module_name_repetitions,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::missing_panics_doc
)]

use std::sync::Arc;
use std::time::Duration;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use genatp_browser::launch;
use genatp_core::budget::{BudgetLimits, Pricing};
use genatp_core::event_log::EventLog;
use genatp_core::orchestrator::{OrchestratorConfig, RunOutcome, run as run_task};
use genatp_core::page_adapter::PageAdapter;
use genatp_core::task::Task;
use genatp_llm::{MockProvider, resolve_from_env};
use genatp_mcp::{McpSession, SessionConfig, tools as mcp_tools};

/// Convert any anyhow / std error into a Python `RuntimeError`.
fn to_pyerr<E: std::fmt::Display>(e: E) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

fn build_runtime() -> PyResult<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(to_pyerr)
}

// ---------------------------------------------------------------------------
// Native types surfaced to Python
// ---------------------------------------------------------------------------

/// Result of [`Agent.run`] — what the agent accomplished, and how.
#[pyclass(module = "genatp._native", get_all, frozen, skip_from_py_object)]
#[derive(Debug, Clone)]
struct NativeRunResult {
    /// `"success"` or `"aborted"`.
    outcome: String,
    /// Final summary the agent produced (success) or the abort reason.
    summary: String,
    /// Step counter at termination.
    steps: u32,
    /// Total prompt+completion tokens charged across all LLM calls.
    tokens: u32,
    /// Cumulative USD spend (zero for free providers like Ollama).
    usd: f64,
    /// Number of events recorded in the run's hash-chained log. Useful
    /// for debugging — the actual log is not surfaced (it would force
    /// us to expose all of genatp-core's event types).
    event_count: usize,
}

#[pymethods]
impl NativeRunResult {
    fn __repr__(&self) -> String {
        format!(
            "NativeRunResult(outcome={:?}, steps={}, tokens={}, usd={:.4})",
            self.outcome, self.steps, self.tokens, self.usd,
        )
    }
}

/// Snapshot of a resolved LLM provider — the auto-detected kind, the
/// default model id, and the provider's stable name.
#[pyclass(module = "genatp._native", get_all, frozen, skip_from_py_object)]
#[derive(Debug, Clone)]
struct NativeProviderInfo {
    /// `"anthropic"`, `"openai"`, `"nim"`, or `"ollama"`.
    kind: String,
    /// Provider's `name()` (matches `kind` for everything except NIM,
    /// which reports `"openai"` because it shares the wire protocol).
    name: String,
    /// Default model the orchestrator will send unless overridden.
    model: String,
}

#[pymethods]
impl NativeProviderInfo {
    fn __repr__(&self) -> String {
        format!(
            "NativeProviderInfo(kind={:?}, name={:?}, model={:?})",
            self.kind, self.name, self.model,
        )
    }
}

/// One MCP tool descriptor — `(name, description, input_schema_json)`.
#[pyclass(module = "genatp._native", get_all, frozen, skip_from_py_object)]
#[derive(Debug, Clone)]
struct NativeToolDescriptor {
    name: String,
    description: String,
    /// JSON-Schema rendered as a string. The Python layer parses it
    /// once on import.
    input_schema_json: String,
}

#[pymethods]
impl NativeToolDescriptor {
    fn __repr__(&self) -> String {
        format!("NativeToolDescriptor(name={:?})", self.name)
    }
}

/// Browser session handle — wraps a single `McpSession` so Python can
/// drive the same MCP tool surface from a process embedding genATP.
///
/// The session owns a Chrome subprocess (spawned lazily on first tool
/// call). Always call [`NativeBrowserSession.close`] when done, or use
/// the SDK's `with`-statement wrapper.
#[pyclass(module = "genatp._native", unsendable)]
struct NativeBrowserSession {
    session: McpSession,
    runtime: Option<tokio::runtime::Runtime>,
}

#[pymethods]
impl NativeBrowserSession {
    #[new]
    #[pyo3(signature = (headless = true, user_data_dir = None))]
    fn new(headless: bool, user_data_dir: Option<String>) -> PyResult<Self> {
        let runtime = build_runtime()?;
        let cfg = SessionConfig {
            headless,
            user_data_dir: user_data_dir.map(std::path::PathBuf::from),
            pool_capacity: 1,
            ..SessionConfig::default()
        };
        let session = runtime.block_on(McpSession::new(cfg)).map_err(to_pyerr)?;
        Ok(Self {
            session,
            runtime: Some(runtime),
        })
    }

    /// Drive the underlying MCP tool dispatcher.
    ///
    /// Mirrors the JSON-RPC `tools/call` surface so the SDK can reuse
    /// the same nine tools the MCP server exposes. Returns a JSON
    /// string (text content) for text tools and a base64 string for
    /// the `screenshot` tool — the Python layer normalises both.
    fn call_tool(&self, name: &str, arguments_json: &str) -> PyResult<(String, bool, String)> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("session closed"))?;
        let args: serde_json::Value = if arguments_json.trim().is_empty() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            serde_json::from_str(arguments_json)
                .map_err(|e| PyValueError::new_err(format!("invalid arguments JSON: {e}")))?
        };
        let result = runtime
            .block_on(mcp_tools::dispatch(&self.session, name, &args))
            .map_err(to_pyerr)?;
        // Serialise the content for Python. We pick the first content
        // part — all v0.1 tools emit exactly one. The Python wrapper
        // already knows which tools return text vs image.
        let (content_text, content_kind) = match result.content.first() {
            Some(genatp_mcp::protocol::ToolContent::Text { text }) => {
                (text.clone(), "text".to_owned())
            }
            Some(genatp_mcp::protocol::ToolContent::Image { data, .. }) => {
                (data.clone(), "image".to_owned())
            }
            None => (String::new(), "text".to_owned()),
        };
        Ok((content_text, result.is_error, content_kind))
    }

    /// Convenience wrapper around the `navigate` tool — returns the
    /// final URL string after redirects.
    fn navigate(&self, url: &str) -> PyResult<String> {
        let args = serde_json::json!({ "url": url }).to_string();
        let (text, is_error, _kind) = self.call_tool("navigate", &args)?;
        if is_error {
            Err(PyRuntimeError::new_err(text))
        } else {
            // `navigate` returns `"navigated to <url>"`; strip the prefix
            // when present, otherwise hand back the raw text.
            Ok(text.trim_start_matches("navigated to ").to_owned())
        }
    }

    /// Shorthand for the `snapshot` tool — returns the JSON string.
    fn snapshot(&self) -> PyResult<String> {
        let (text, is_error, _kind) = self.call_tool("snapshot", "")?;
        if is_error {
            Err(PyRuntimeError::new_err(text))
        } else {
            Ok(text)
        }
    }

    /// Close the session — must be called explicitly. Idempotent.
    fn close(&mut self) -> PyResult<()> {
        self.runtime = None;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Return the crate version (matches `genatp.__version__`).
#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Resolve a provider from environment variables. Returns
/// [`NativeProviderInfo`].
#[pyfunction]
fn resolve_provider_from_env() -> PyResult<NativeProviderInfo> {
    let resolved = resolve_from_env().map_err(to_pyerr)?;
    Ok(NativeProviderInfo {
        kind: resolved.kind.as_str().to_owned(),
        name: resolved.provider.name().to_owned(),
        model: resolved.model,
    })
}

/// Return the canonical list of MCP tool descriptors. The Python
/// `genatp.tools` module turns these into LangChain / LlamaIndex
/// tools.
#[pyfunction]
fn list_mcp_tools() -> PyResult<Vec<NativeToolDescriptor>> {
    let mut out = Vec::new();
    for d in genatp_mcp::tools::descriptors() {
        out.push(NativeToolDescriptor {
            name: d.name,
            description: d.description,
            input_schema_json: serde_json::to_string(&d.input_schema).map_err(to_pyerr)?,
        });
    }
    Ok(out)
}

/// Run a single agent task. This is the heart of the Python SDK.
///
/// Parameters:
///   goal:        plain-English task description.
///   start_url:   URL the agent navigates to first.
///   model:       LLM model id (defaults to the resolved provider's default).
///   max_steps:   step ceiling. Independent of token/wall budgets.
///   max_tokens:  hard token budget (0 = unlimited).
///   max_wall_s:  wall-clock budget in seconds (0 = unlimited).
///   max_usd:     dollar budget (0 = unlimited).
///   task_id:     event-log key; defaults to a uuid-like timestamp.
///   headless:    Chrome headless mode.
///   user_data_dir: persistent profile dir; `None` for ephemeral.
///   provider:    `"anthropic" | "openai" | "nim" | "ollama" | "mock"`.
///                `None` auto-detects via env vars (see Phase-3 factory).
#[pyfunction]
#[pyo3(signature = (
    goal,
    start_url,
    model = None,
    max_steps = 30,
    max_tokens = 0,
    max_wall_s = 0,
    max_usd = 0.0,
    system_prompt = None,
    task_id = None,
    headless = true,
    user_data_dir = None,
    provider = None,
    mock_reply = None,
))]
#[allow(clippy::too_many_arguments)]
fn run_agent(
    goal: &str,
    start_url: &str,
    model: Option<String>,
    max_steps: u32,
    max_tokens: u32,
    max_wall_s: u64,
    max_usd: f64,
    system_prompt: Option<String>,
    task_id: Option<String>,
    headless: bool,
    user_data_dir: Option<String>,
    provider: Option<String>,
    mock_reply: Option<String>,
) -> PyResult<NativeRunResult> {
    let runtime = build_runtime()?;
    let outcome = runtime.block_on(async move {
        // 1) Provider.
        let (provider_arc, resolved_model): (Arc<dyn genatp_llm::LlmProvider>, String) =
            match provider.as_deref() {
                Some("mock") => {
                    let reply = mock_reply.unwrap_or_else(|| "done".to_owned());
                    let p = MockProvider::with_reply(reply);
                    (Arc::new(p), model.unwrap_or_else(|| "mock".to_owned()))
                }
                Some(other) => {
                    let kind = genatp_llm::ProviderKind::parse(other).map_err(to_pyerr)?;
                    let resolved = genatp_llm::build_provider(kind).map_err(to_pyerr)?;
                    let m = model.unwrap_or(resolved.model);
                    (resolved.provider, m)
                }
                None => {
                    let resolved = resolve_from_env().map_err(to_pyerr)?;
                    let m = model.unwrap_or(resolved.model);
                    (resolved.provider, m)
                }
            };

        // 2) Browser.
        let browser = launch(genatp_browser::LaunchOptions {
            headless: if headless {
                genatp_browser::launch::HeadlessMode::New
            } else {
                genatp_browser::launch::HeadlessMode::Visible
            },
            user_data_dir: user_data_dir.map(std::path::PathBuf::from),
            ..genatp_browser::LaunchOptions::default()
        })
        .await
        .map_err(to_pyerr)?;
        let page = browser.new_page().await.map_err(to_pyerr)?;

        // 3) Task.
        let id = task_id.unwrap_or_else(|| format!("py-{}", now_ms()));
        let task = Task {
            id,
            goal: goal.to_owned(),
            start_url: start_url.to_owned(),
            model: resolved_model.clone(),
            system_prompt,
            max_steps,
            budget: BudgetLimits {
                max_tokens,
                max_wall: Duration::from_secs(max_wall_s),
                max_usd,
                ..BudgetLimits::default()
            },
            pricing: Pricing::FREE,
            retry: genatp_core::RetryPolicy::default(),
        };

        // 4) Run.
        let mut log = EventLog::in_memory();
        let result = run_task(OrchestratorConfig {
            task,
            browser: PageAdapter::new(page),
            provider: provider_arc,
            event_log: &mut log,
            snapshot_store: None,
            policy_engine: genatp_perception::PolicyEngine::default(),
            vision_provider: None,
        })
        .await;

        let event_count = log.entries().len();

        // 5) Translate into a Python-friendly result.
        let last_spend = log.entries().iter().rev().find_map(|entry| {
            if let genatp_core::event_log::Event::RunFinished { spend, .. } = &entry.event {
                Some(*spend)
            } else {
                None
            }
        });
        let (tokens, usd) = last_spend.map(|s| (s.tokens, s.usd)).unwrap_or((0, 0.0));

        let value = match result {
            Ok(RunOutcome::Success { summary, steps }) => NativeRunResult {
                outcome: "success".into(),
                summary,
                steps,
                tokens,
                usd,
                event_count,
            },
            Ok(RunOutcome::Aborted { reason, steps }) => NativeRunResult {
                outcome: "aborted".into(),
                summary: reason,
                steps,
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

        let _ = browser
            .shutdown(Duration::from_secs(5))
            .await
            .map_err(|e| tracing::warn!(error = %e, "browser shutdown timed out"));

        Ok::<NativeRunResult, PyErr>(value)
    })?;
    Ok(outcome)
}

/// Diagnostics helper — returns a Python dict describing the build:
/// crate versions, supported MCP tools, supported provider kinds.
/// Used by `genatp doctor` and by the test suite to sanity-check the
/// wheel.
#[pyfunction]
fn build_info(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("genatp_python", env!("CARGO_PKG_VERSION"))?;
    dict.set_item("genatp_core", genatp_core::version())?;
    dict.set_item("genatp_llm", genatp_llm::version())?;
    dict.set_item("genatp_browser", genatp_browser::version())?;
    dict.set_item("genatp_perception", genatp_perception::version())?;
    dict.set_item("genatp_mcp", genatp_mcp::version())?;
    let tools = PyList::empty(py);
    for d in genatp_mcp::tools::descriptors() {
        tools.append(d.name)?;
    }
    dict.set_item("mcp_tools", tools)?;
    let providers = PyList::empty(py);
    for k in ["anthropic", "openai", "nim", "ollama", "mock"] {
        providers.append(k)?;
    }
    dict.set_item("providers", providers)?;
    Ok(dict)
}

/// Milliseconds since the Unix epoch — used to suffix default task ids.
fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// PyO3 module glue
// ---------------------------------------------------------------------------

/// Python module init — the symbol Python loads as `genatp._native`.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<NativeRunResult>()?;
    m.add_class::<NativeProviderInfo>()?;
    m.add_class::<NativeToolDescriptor>()?;
    m.add_class::<NativeBrowserSession>()?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_provider_from_env, m)?)?;
    m.add_function(wrap_pyfunction!(list_mcp_tools, m)?)?;
    m.add_function(wrap_pyfunction!(run_agent, m)?)?;
    m.add_function(wrap_pyfunction!(build_info, m)?)?;
    Ok(())
}
