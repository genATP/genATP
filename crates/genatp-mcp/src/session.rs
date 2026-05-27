//! Per-connection state held by the MCP server.
//!
//! An [`McpSession`] owns the browser handle (one tab per session) and
//! tracks the latest perception snapshot so tools that reference
//! elements via `@eN` work consistently across calls.
//!
//! ## Lifetime
//!
//! - **stdio transport** — exactly one session per process lifetime
//!   (Claude Desktop / Cursor spawn one server subprocess per server).
//! - **HTTP transport** — currently one **shared** session per server
//!   process (we treat the HTTP transport as a single-tenant
//!   localhost-only endpoint for v0.1.0). Multi-tenant HTTP arrives in
//!   Phase 10.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use genatp_browser::launch::HeadlessMode;
use genatp_browser::pool::{BrowserPool, PageGuard, PoolConfig};
use genatp_browser::{AccessibilityTree, LaunchOptions};
use genatp_core::perception_bridge::raw_tree_from_accessibility;
use genatp_perception::{CompressOptions, Ref, RefMap, StripPolicy, compress, strip};

use crate::error::{McpError, Result};

/// Configuration for spawning the browser bound to this session.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Headless mode toggle. Defaults to `true` so the server can run
    /// in CI / Docker without a display server.
    pub headless: bool,
    /// User-data directory. `None` uses a fresh temp directory each run
    /// (recommended for ephemeral MCP sessions); pass `Some(path)` to
    /// reuse cookies / localStorage across runs.
    pub user_data_dir: Option<PathBuf>,
    /// Maximum concurrent pages (v0.1.0 only allocates one).
    pub pool_capacity: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            headless: true,
            user_data_dir: None,
            pool_capacity: 1,
        }
    }
}

/// Per-session state. Cheaply clonable (`Arc<Mutex<...>>` inside).
#[derive(Clone)]
pub struct McpSession {
    inner: Arc<Mutex<SessionInner>>,
}

impl std::fmt::Debug for McpSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpSession").finish_non_exhaustive()
    }
}

struct SessionInner {
    /// Active browser pool. Held for the session lifetime so the
    /// Chrome subprocess stays alive between tool calls.
    pool: BrowserPool,
    /// The active page guard. Acquired lazily on first tool call so
    /// `initialize` is cheap (no Chrome spawn until the agent actually
    /// asks for a page).
    page: Option<PageGuard>,
    /// Stable identity → ref map; threaded through every snapshot so
    /// `@eN` refs do not rotate spuriously.
    refmap: RefMap,
    /// Ref → backend DOM node id, populated from the most recent
    /// snapshot. `click`/`fill` resolve `@eN` through this table.
    ref_to_backend: HashMap<Ref, String>,
    /// Most recent compressed snapshot text (cached for
    /// `resources/read`).
    last_snapshot_json: Option<String>,
    /// Most recent accessibility tree (used for `extract`).
    last_tree: Option<AccessibilityTree>,
}

impl McpSession {
    /// Build a session backed by a fresh browser pool. Does NOT spawn
    /// Chrome — that happens on the first tool call.
    ///
    /// # Errors
    /// Returns [`McpError::Browser`] if the pool fails to initialise
    /// (e.g. invalid Chrome path).
    pub async fn new(cfg: SessionConfig) -> Result<Self> {
        let launch = LaunchOptions {
            headless: if cfg.headless {
                HeadlessMode::New
            } else {
                HeadlessMode::Visible
            },
            user_data_dir: cfg.user_data_dir.clone(),
            ..LaunchOptions::default()
        };
        let pool_cfg = PoolConfig {
            launch,
            ..PoolConfig::default()
        }
        .max_concurrency(cfg.pool_capacity.max(1));
        let pool = BrowserPool::new(pool_cfg);
        Ok(Self {
            inner: Arc::new(Mutex::new(SessionInner {
                pool,
                page: None,
                refmap: RefMap::new(),
                ref_to_backend: HashMap::new(),
                last_snapshot_json: None,
                last_tree: None,
            })),
        })
    }

    /// Lazily acquire the page guard if not already held. Caller MUST
    /// be holding the inner mutex.
    async fn ensure_page(inner: &mut SessionInner) -> Result<()> {
        if inner.page.is_none() {
            let page = inner.pool.acquire().await?;
            inner.page = Some(page);
        }
        Ok(())
    }

    /// Navigate the bound tab to `url`. Returns the resolved final URL
    /// (after redirects).
    ///
    /// # Errors
    /// Returns [`McpError::Browser`] on CDP / timeout failures.
    pub async fn navigate(&self, url: &str) -> Result<String> {
        let mut inner = self.inner.lock().await;
        Self::ensure_page(&mut inner).await?;
        let page = inner.page.as_ref().expect("page acquired").page();
        page.navigate(url).await?;
        let final_url = page.url().await?.unwrap_or_else(|| url.to_string());
        Ok(final_url)
    }

    /// Capture a fresh perception snapshot, update the session's
    /// ref→backend map so subsequent `click`/`fill` calls resolve
    /// correctly, and return the JSON-serialised snapshot text.
    ///
    /// # Errors
    /// Returns [`McpError::Browser`] on a11y fetch failures and
    /// [`McpError::Perception`] on compression failures.
    pub async fn snapshot(&self) -> Result<String> {
        let mut inner = self.inner.lock().await;
        Self::ensure_page(&mut inner).await?;
        let page = inner.page.as_ref().expect("page acquired").page();
        let url = page.url().await?.unwrap_or_default();
        let tree = page.accessibility_tree().await?;
        let raw = raw_tree_from_accessibility(&url, &tree);

        let a11y_to_backend: HashMap<String, String> = tree
            .nodes()
            .iter()
            .filter_map(|n| {
                n.backend_dom_node_id
                    .map(|b| (n.node_id.clone(), b.to_string()))
            })
            .collect();

        let snapshot = compress(&raw, &mut inner.refmap, &CompressOptions::default());
        // `compress` strips + assigns internally, but does not expose the
        // node_id → ref map. Re-run strip+assign to recover it; the
        // shared `RefMap` makes this idempotent (existing identities
        // reuse their refs).
        let stripped = strip(&raw, &StripPolicy::default());
        let node_to_ref = inner.refmap.assign_for_tree(&stripped);

        let mut ref_to_backend: HashMap<Ref, String> = HashMap::new();
        for (a11y_node_id, r) in node_to_ref {
            if let Some(backend) = a11y_to_backend.get(&a11y_node_id) {
                ref_to_backend.insert(r, backend.clone());
            }
        }
        let json = serde_json::to_string(&snapshot)?;
        inner.ref_to_backend = ref_to_backend;
        inner.last_snapshot_json = Some(json.clone());
        inner.last_tree = Some(tree);
        Ok(json)
    }

    /// Resolve a perception `@eN` ref against the latest snapshot and
    /// return the backend DOM node id as a string suitable for
    /// `Page::click_node` / `Page::fill_node`.
    fn ref_to_backend_id(inner: &SessionInner, refstr: &str) -> Result<String> {
        if !refstr.starts_with("@e") {
            return Err(McpError::InvalidParams(format!(
                "ref must look like `@eN`, got `{refstr}`"
            )));
        }
        inner.ref_to_backend.get(refstr).cloned().ok_or_else(|| {
            McpError::ToolFailed(format!(
                "ref `{refstr}` not in current snapshot — call `snapshot` first"
            ))
        })
    }

    /// Click an element previously identified in the snapshot.
    ///
    /// # Errors
    /// Returns [`McpError::ToolFailed`] if the ref is not in the
    /// current snapshot; [`McpError::Browser`] on click execution
    /// failures.
    pub async fn click(&self, refstr: &str) -> Result<()> {
        let mut inner = self.inner.lock().await;
        Self::ensure_page(&mut inner).await?;
        let backend = Self::ref_to_backend_id(&inner, refstr)?;
        let page = inner.page.as_ref().expect("page acquired").page();
        page.click_node(&backend).await?;
        Ok(())
    }

    /// Fill a text input previously identified in the snapshot.
    ///
    /// # Errors
    /// As for [`Self::click`].
    pub async fn fill(&self, refstr: &str, value: &str) -> Result<()> {
        let mut inner = self.inner.lock().await;
        Self::ensure_page(&mut inner).await?;
        let backend = Self::ref_to_backend_id(&inner, refstr)?;
        let page = inner.page.as_ref().expect("page acquired").page();
        page.fill_node(&backend, value).await?;
        Ok(())
    }

    /// Scroll the viewport by relative pixels.
    ///
    /// # Errors
    /// Returns [`McpError::Browser`] on CDP failures.
    pub async fn scroll(&self, dx: i32, dy: i32) -> Result<()> {
        let mut inner = self.inner.lock().await;
        Self::ensure_page(&mut inner).await?;
        let page = inner.page.as_ref().expect("page acquired").page();
        page.scroll_by(dx, dy).await?;
        Ok(())
    }

    /// Capture a full-viewport PNG screenshot, returned as raw bytes.
    /// The MCP layer base64-encodes for the `image` content part.
    ///
    /// # Errors
    /// Returns [`McpError::Browser`] on CDP failures.
    pub async fn screenshot_png(&self) -> Result<Vec<u8>> {
        let mut inner = self.inner.lock().await;
        Self::ensure_page(&mut inner).await?;
        let page = inner.page.as_ref().expect("page acquired").page();
        Ok(page.screenshot_png().await?)
    }

    /// Return the bound tab's current URL.
    ///
    /// # Errors
    /// Returns [`McpError::Browser`] on CDP failures.
    pub async fn current_url(&self) -> Result<String> {
        let mut inner = self.inner.lock().await;
        Self::ensure_page(&mut inner).await?;
        let page = inner.page.as_ref().expect("page acquired").page();
        Ok(page.url().await?.unwrap_or_default())
    }

    /// Navigate back in the bound tab's history. Returns `true` if the
    /// navigation actually happened.
    ///
    /// # Errors
    /// Returns [`McpError::Browser`] on CDP failures.
    pub async fn go_back(&self) -> Result<bool> {
        let mut inner = self.inner.lock().await;
        Self::ensure_page(&mut inner).await?;
        let page = inner.page.as_ref().expect("page acquired").page();
        Ok(page.go_back().await?)
    }

    /// Extract visible text from the most recent snapshot, optionally
    /// filtered by a case-insensitive substring `query`. Returns one
    /// line per matching node so the LLM can quote it back to the
    /// user.
    ///
    /// Note: extract reads from the **cached** snapshot — callers
    /// should invoke `snapshot` first, or after any navigation. If no
    /// snapshot has been captured yet, returns an empty string.
    pub async fn extract(&self, query: Option<&str>) -> Result<String> {
        let guard = self.inner.lock().await;
        let Some(tree) = guard.last_tree.as_ref() else {
            return Ok(String::new());
        };
        let q = query.map(str::to_lowercase);
        let mut out = String::new();
        for node in tree.nodes() {
            if node.ignored {
                continue;
            }
            let mut bits: Vec<String> = Vec::new();
            if let Some(name) = node.name.as_ref().filter(|s| !s.is_empty()) {
                bits.push(name.clone());
            }
            if let Some(value) = node.value.as_ref().filter(|s| !s.is_empty()) {
                bits.push(value.clone());
            }
            if bits.is_empty() {
                continue;
            }
            let line = bits.join(" — ");
            if let Some(ref q_lc) = q
                && !line.to_lowercase().contains(q_lc)
            {
                continue;
            }
            out.push_str(&line);
            out.push('\n');
        }
        Ok(out)
    }

    /// Return the most recent compressed snapshot JSON, if any. Used
    /// by the `resources/read` handler to surface the snapshot as a
    /// readable MCP resource.
    pub async fn last_snapshot_json(&self) -> Option<String> {
        self.inner.lock().await.last_snapshot_json.clone()
    }
}
