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
use genatp_browser::storage::{
    Cookie, dump_cookies, dump_local_storage, dump_session_storage, set_cookies, set_local_storage,
    set_session_storage,
};
use genatp_browser::{AccessibilityTree, LaunchOptions};
use genatp_core::perception_bridge::raw_tree_from_accessibility;
use genatp_perception::{CompressOptions, Ref, RefMap, StripPolicy, compress, strip};
use genatp_vault::{Capability, Entry, KeyStore, OsKeyStore, Vault};

use crate::error::{McpError, Result};

/// Result of [`McpSession::save_auth`]: counts plus the cookie names
/// captured (no values) so the tool layer can surface a useful summary
/// to the LLM. The skipped list helps debug domain-filter mismatches.
#[derive(Debug, Default, Clone)]
pub struct SaveAuthSummary {
    /// Cookies kept after domain filtering.
    pub cookies: usize,
    /// `localStorage` entries kept.
    pub local_storage: usize,
    /// `sessionStorage` entries kept.
    pub session_storage: usize,
    /// Names of cookies that were stored.
    pub cookie_names: Vec<String>,
    /// `name@domain` of cookies skipped because their domain did not
    /// match the requested target. Empty in the happy path.
    pub skipped_cookie_names: Vec<String>,
}

impl SaveAuthSummary {
    /// Total count of stored entries.
    #[must_use]
    pub fn total(&self) -> usize {
        self.cookies + self.local_storage + self.session_storage
    }
}

/// Result of [`McpSession::load_auth`].
#[derive(Debug, Default, Clone)]
pub struct LoadAuthSummary {
    /// Cookies hydrated.
    pub cookies: usize,
    /// `localStorage` entries hydrated.
    pub local_storage: usize,
    /// `sessionStorage` entries hydrated.
    pub session_storage: usize,
}

impl LoadAuthSummary {
    /// Total count of hydrated entries.
    #[must_use]
    pub fn total(&self) -> usize {
        self.cookies + self.local_storage + self.session_storage
    }
}

/// Configuration for spawning the browser bound to this session.
#[derive(Clone)]
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
    /// Vault file path. `None` falls back to the per-user default
    /// (`directories::ProjectDirs`) at first auth-tool use.
    pub vault_path: Option<PathBuf>,
    /// Auth Vault keystore. `None` ⇒ OS keychain (production default).
    /// Tests override with [`genatp_vault::InMemoryKeyStore`].
    pub vault_keystore: Option<Arc<dyn KeyStore>>,
    /// Capability granted to vault tools. Defaults to `Capability::any()`
    /// (interactive use). The CLI `serve` subcommand can constrain
    /// this to only the domains a cassette declared.
    pub vault_capability: Capability,
}

impl std::fmt::Debug for SessionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionConfig")
            .field("headless", &self.headless)
            .field("user_data_dir", &self.user_data_dir)
            .field("pool_capacity", &self.pool_capacity)
            .field("vault_path", &self.vault_path)
            .field("vault_keystore", &self.vault_keystore.is_some())
            .field(
                "vault_capability_scoped",
                &self.vault_capability.domains().is_some(),
            )
            .finish()
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            headless: true,
            user_data_dir: None,
            pool_capacity: 1,
            vault_path: None,
            vault_keystore: None,
            vault_capability: Capability::any(),
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
    /// Lazy-opened auth vault. Held across tool calls so the keychain
    /// prompt fires at most once per session.
    vault: Option<Vault>,
    /// Where to open the vault on first use.
    vault_path: Option<PathBuf>,
    /// Keystore custodian for the master KEK. `None` ⇒ OS keychain.
    vault_keystore: Option<Arc<dyn KeyStore>>,
    /// Capability gate for `save_auth` / `load_auth`.
    vault_capability: Capability,
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
                vault: None,
                vault_path: cfg.vault_path,
                vault_keystore: cfg.vault_keystore,
                vault_capability: cfg.vault_capability,
            })),
        })
    }

    /// Lazily acquire the page guard if not already held. Caller MUST
    /// be holding the inner mutex.
    ///
    /// On Chrome 148+ the CDP WebSocket can drop unexpectedly after
    /// navigation (`ResetWithoutClosingHandshake`). This method detects
    /// a dead WS by issuing a cheap `url()` call; if the receiver is gone
    /// it automatically releases the stale page, reconnects the browser
    /// pool, and opens a fresh tab so subsequent tools work transparently.
    async fn ensure_page(inner: &mut SessionInner) -> Result<()> {
        if inner.page.is_some() {
            // Health-check: is the CDP WebSocket still alive?
            let page = inner.page.as_ref().unwrap().page();
            match page.url().await {
                Ok(_) => return Ok(()), // WS is alive, keep the current page
                Err(e) if Self::is_dead_connection(&e) => {
                    tracing::warn!(
                        target = "genatp::session",
                        "CDP WebSocket disconnected — reconnecting browser"
                    );
                    // Drop the stale page guard.
                    inner.page = None;
                    // Re-launch Chrome.
                    inner.pool.reconnect().await?;
                    // Acquire a fresh page from the new Chrome.
                    let fresh = inner.pool.acquire().await?;
                    inner.page = Some(fresh);
                    return Ok(());
                }
                Err(e) => return Err(McpError::Browser(e)), // propagate other errors
            }
        }
        // No page yet — acquire one.
        let page = inner.pool.acquire().await?;
        inner.page = Some(page);
        Ok(())
    }

    /// True if the error indicates the CDP WebSocket receiver channel
    /// is gone (Chrome closed the connection).
    fn is_dead_connection(err: &genatp_browser::BrowserError) -> bool {
        err.to_string().contains("receiver is gone") || err.to_string().contains("channel closed")
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

    /// Click at absolute viewport coordinates. Used in vision mode
    /// when @eN refs are unavailable.
    ///
    /// # Errors
    /// Returns [`McpError::Browser`] on CDP failures.
    pub async fn click_coordinate(&self, x: f64, y: f64) -> Result<()> {
        let mut inner = self.inner.lock().await;
        Self::ensure_page(&mut inner).await?;
        let page = inner.page.as_ref().expect("page acquired").page();
        page.click_coordinate(x, y).await?;
        Ok(())
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

    // --- Auth Vault (Phase 6) -----------------------------------

    /// Lazily open the bound auth vault. Caller must hold the inner
    /// mutex.
    fn ensure_vault(inner: &mut SessionInner) -> Result<&mut Vault> {
        if inner.vault.is_none() {
            let path = match inner.vault_path.clone() {
                Some(p) => p,
                None => genatp_vault::default_vault_path()
                    .map_err(|e| McpError::Internal(format!("vault path: {e}")))?,
            };
            let ks: Arc<dyn KeyStore> = inner
                .vault_keystore
                .clone()
                .unwrap_or_else(|| Arc::new(OsKeyStore::new()));
            let vault = Vault::open_or_init(&path, ks)
                .map_err(|e| McpError::Internal(format!("vault open: {e}")))?;
            inner.vault = Some(vault);
        }
        Ok(inner
            .vault
            .as_mut()
            .expect("vault was just initialized above"))
    }

    /// Save the current page's cookies + `localStorage` for `domain`
    /// into the auth vault.
    ///
    /// # Errors
    /// Returns [`McpError::ToolFailed`] for capability denials and
    /// [`McpError::Internal`] for vault failures.
    pub async fn save_auth(&self, domain: &str) -> Result<SaveAuthSummary> {
        let mut inner = self.inner.lock().await;
        Self::ensure_page(&mut inner).await?;
        let page = inner.page.as_ref().expect("page acquired").page();

        let raw_cookies = dump_cookies(page, &[]).await?;
        let local = dump_local_storage(page).await?;
        let session = dump_session_storage(page).await?;

        // Diagnostic logging: names only, never values. Visible at
        // RUST_LOG=info or via the genatp serve --verbose flag.
        let cdp_cookie_names: Vec<String> = raw_cookies
            .iter()
            .map(|c| {
                format!(
                    "{}@{}",
                    c.name,
                    c.domain.as_deref().unwrap_or("(no-domain)")
                )
            })
            .collect();
        tracing::info!(
            target = "genatp::vault",
            domain,
            cdp_cookie_count = raw_cookies.len(),
            local_storage_keys = local.len(),
            session_storage_keys = session.len(),
            cdp_cookies = ?cdp_cookie_names,
            local_storage = ?local.iter().map(|(k, _)| k).collect::<Vec<_>>(),
            session_storage = ?session.iter().map(|(k, _)| k).collect::<Vec<_>>(),
            "save_auth — captured (pre-filter)"
        );

        let mut entries: Vec<Entry> = Vec::new();
        let mut kept_cookie_names: Vec<String> = Vec::new();
        let mut skipped_cookie_names: Vec<String> = Vec::new();
        for c in raw_cookies {
            // Filter to the requested domain only (CDP returns every
            // cookie known to the context — we want session isolation).
            if !cookie_matches_domain(&c, domain) {
                skipped_cookie_names.push(format!(
                    "{}@{}",
                    c.name,
                    c.domain.as_deref().unwrap_or("(no-domain)")
                ));
                continue;
            }
            kept_cookie_names.push(c.name.clone());
            if let Ok(e) = Entry::cookie(domain, c) {
                entries.push(e);
            }
        }
        for (k, v) in local {
            if let Ok(e) = Entry::local_storage(domain, k, v) {
                entries.push(e);
            }
        }
        for (k, v) in session {
            if let Ok(e) = Entry::session_storage(domain, k, v) {
                entries.push(e);
            }
        }

        let cookies_kept = kept_cookie_names.len();
        let local_kept = entries
            .iter()
            .filter(|e| matches!(e, Entry::LocalStorage { .. }))
            .count();
        let session_kept = entries
            .iter()
            .filter(|e| matches!(e, Entry::SessionStorage { .. }))
            .count();

        tracing::info!(
            target = "genatp::vault",
            domain,
            cookies_kept,
            local_kept,
            session_kept,
            kept_cookies = ?kept_cookie_names,
            skipped_cookies = ?skipped_cookie_names,
            "save_auth — after domain filter"
        );

        if entries.is_empty() {
            return Ok(SaveAuthSummary::default());
        }

        let cap = inner.vault_capability.clone();
        let vault = Self::ensure_vault(&mut inner)?;
        vault.save(&cap, &entries).map_err(|e| match e {
            genatp_vault::VaultError::CapabilityDenied(d) => {
                McpError::ToolFailed(format!("capability denied for `{d}`"))
            }
            other => McpError::Internal(format!("vault save: {other}")),
        })?;
        Ok(SaveAuthSummary {
            cookies: cookies_kept,
            local_storage: local_kept,
            session_storage: session_kept,
            cookie_names: kept_cookie_names,
            skipped_cookie_names,
        })
    }

    /// Hydrate the current page with cookies + `localStorage` +
    /// `sessionStorage` stored under `domain` in the vault. The page
    /// should already have navigated to a same-origin URL so Chrome
    /// accepts the cookies.
    ///
    /// Returns a summary of what was restored.
    ///
    /// # Errors
    /// As for [`Self::save_auth`].
    pub async fn load_auth(&self, domain: &str) -> Result<LoadAuthSummary> {
        let mut inner = self.inner.lock().await;
        Self::ensure_page(&mut inner).await?;
        let cap = inner.vault_capability.clone();
        let vault = Self::ensure_vault(&mut inner)?;
        let entries = vault.load(&cap, domain).map_err(|e| match e {
            genatp_vault::VaultError::CapabilityDenied(d) => {
                McpError::ToolFailed(format!("capability denied for `{d}`"))
            }
            other => McpError::Internal(format!("vault load: {other}")),
        })?;
        let count = entries.len();
        if count == 0 {
            return Ok(LoadAuthSummary::default());
        }

        let page = inner.page.as_ref().expect("page acquired").page();
        let mut cookies: Vec<Cookie> = Vec::new();
        let mut cookie_names: Vec<String> = Vec::new();
        let mut local_keys: Vec<String> = Vec::new();
        let mut session_keys: Vec<String> = Vec::new();
        for e in entries {
            match e {
                Entry::Cookie { cookie, .. } => {
                    cookie_names.push(cookie.name.clone());
                    cookies.push(cookie);
                }
                Entry::LocalStorage { key, value, .. } => {
                    local_keys.push(key.clone());
                    set_local_storage(page, &key, &value).await?;
                }
                Entry::SessionStorage { key, value, .. } => {
                    session_keys.push(key.clone());
                    set_session_storage(page, &key, &value).await?;
                }
            }
        }
        if !cookies.is_empty() {
            set_cookies(page, cookies).await?;
        }

        tracing::info!(
            target = "genatp::vault",
            domain,
            cookies = cookie_names.len(),
            local_storage = local_keys.len(),
            session_storage = session_keys.len(),
            cookie_names = ?cookie_names,
            local_keys = ?local_keys,
            session_keys = ?session_keys,
            "load_auth — hydrated"
        );

        // Many sites only read session cookies on the next navigation.
        // Reload in-place so the user does not need a second `navigate`.
        let _ = page.raw().evaluate("location.reload()").await;

        Ok(LoadAuthSummary {
            cookies: cookie_names.len(),
            local_storage: local_keys.len(),
            session_storage: session_keys.len(),
        })
    }

    /// Enumerate every domain stored in the vault.
    ///
    /// # Errors
    /// Returns [`McpError::Internal`] on vault failures.
    pub async fn list_auth(&self) -> Result<Vec<(String, usize)>> {
        let mut inner = self.inner.lock().await;
        let vault = Self::ensure_vault(&mut inner)?;
        let summary = vault
            .list()
            .map_err(|e| McpError::Internal(format!("vault list: {e}")))?;
        Ok(summary
            .into_iter()
            .map(|s| (s.domain, s.entry_count))
            .collect())
    }

    /// Revoke (drop) every entry under `domain`. Returns the rows
    /// removed (always 0 or 1 because we keep one row per domain).
    ///
    /// # Errors
    /// As for [`Self::list_auth`].
    pub async fn revoke_auth(&self, domain: &str) -> Result<usize> {
        let mut inner = self.inner.lock().await;
        let vault = Self::ensure_vault(&mut inner)?;
        vault
            .revoke(domain)
            .map_err(|e| McpError::Internal(format!("vault revoke: {e}")))
    }
}

/// `true` if `cookie.domain` matches `target` after canonicalisation.
/// CDP returns cookie domains with or without a leading dot —
/// `genatp_vault::canonical_domain` normalises both forms.
fn cookie_matches_domain(cookie: &Cookie, target: &str) -> bool {
    let target = match genatp_vault::canonical_domain(target) {
        Ok(d) => d,
        Err(_) => return false,
    };
    let Some(cd) = cookie.domain.as_deref() else {
        // Cookies set via `url=...` have no explicit domain. We
        // optimistically include them — the caller will navigate to a
        // same-origin URL before save_auth fires.
        return true;
    };
    let Ok(cd_canon) = genatp_vault::canonical_domain(cd) else {
        return false;
    };
    // Allow exact match or any sub-domain of `target`.
    cd_canon == target
        || cd_canon.ends_with(&format!(".{target}"))
        || target.ends_with(&format!(".{cd_canon}"))
}
