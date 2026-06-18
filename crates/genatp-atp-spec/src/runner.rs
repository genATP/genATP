//! Runner — executes an [`AtpSpec`] IR against a browser.
//!
//! ## Execution model
//!
//! 1. Open a browser page (via the pool).
//! 2. Navigate to the spec's starting URL (first step, or base_url).
//! 3. For each step:
//!    - Execute the action (click, fill, assert, etc.).
//!    - Record success/failure in a [`StepResult`].
//!    - For assertions, check the state immediately.
//!    - For cassette-backed steps, replay from cassette first.
//! 4. Return a [`SpecResult`] with all step results.
//!
//! The runner integrates with the existing orchestrator infrastructure
//! (`genatp-core::dispatcher::BrowserAdapter`) and the cassette system
//! (`genatp-cassette`).

use std::path::Path;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use genatp_browser::{AccessibilityTree, LaunchOptions, PoolConfig, pool::BrowserPool};
use genatp_core::dispatcher::BrowserAdapter;
use genatp_core::error::CoreError;
use tokio::sync::Mutex;

use crate::error::{AtpSpecError, Result};
use crate::ir::{AtpSpec, ElementSelector, SpecResult, Step, StepResult};

/// Default per-step timeout.
const DEFAULT_STEP_TIMEOUT: Duration = Duration::from_secs(30);

/// A handle to the running browser page, wrapped for safe concurrent access.
struct PageHandle {
    /// Browser pool. Owned as `Option` so `close()` can take ownership
    /// and properly shut down the Chrome subprocess.
    pool: Option<BrowserPool>,
    guard: Mutex<Option<genatp_browser::pool::PageGuard>>,
}

impl std::fmt::Debug for PageHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageHandle")
            .field(
                "has_guard",
                &self.guard.try_lock().map(|g| g.is_some()).unwrap_or(false),
            )
            .finish()
    }
}

impl PageHandle {
    async fn new(launch_opts: LaunchOptions) -> Result<Self> {
        let pool_cfg = PoolConfig::default().launch(launch_opts);
        let pool = BrowserPool::new(pool_cfg);
        let guard = pool
            .acquire()
            .await
            .map_err(|e| AtpSpecError::Internal(format!("failed to acquire browser page: {e}")))?;
        Ok(Self {
            pool: Some(pool),
            guard: Mutex::new(Some(guard)),
        })
    }

    async fn page(&self) -> Result<genatp_browser::Page> {
        let guard = self.guard.lock().await;
        let page = guard
            .as_ref()
            .ok_or_else(|| AtpSpecError::Internal("page guard was taken".into()))?
            .page()
            .clone();
        Ok(page)
    }

    async fn close(&mut self) -> Result<()> {
        // Close the page first.
        let guard = self.guard.lock().await.take();
        if let Some(g) = guard {
            g.close()
                .await
                .map_err(|e| AtpSpecError::Internal(format!("failed to close page: {e}")))?;
        }
        // Then shut down the browser pool (Chrome subprocess).
        if let Some(pool) = self.pool.take() {
            pool.shutdown()
                .await
                .map_err(|e| AtpSpecError::Internal(format!("failed to shutdown browser: {e}")))?;
        }
        Ok(())
    }
}

/// Browser adapter wrapping the page handle for the runner.
#[derive(Debug)]
struct RunnerBrowser {
    handle: PageHandle,
}

#[async_trait]
impl BrowserAdapter for RunnerBrowser {
    async fn navigate(&self, url: &str) -> std::result::Result<(), CoreError> {
        self.handle
            .page()
            .await
            .map_err(|e| CoreError::Browser(e.to_string()))?
            .navigate(url)
            .await?;
        Ok(())
    }

    async fn click_node(&self, node_id: &str) -> std::result::Result<(), CoreError> {
        self.handle
            .page()
            .await
            .map_err(|e| CoreError::Browser(e.to_string()))?
            .click_node(node_id)
            .await?;
        Ok(())
    }

    async fn fill_node(&self, node_id: &str, value: &str) -> std::result::Result<(), CoreError> {
        self.handle
            .page()
            .await
            .map_err(|e| CoreError::Browser(e.to_string()))?
            .fill_node(node_id, value)
            .await?;
        Ok(())
    }

    async fn press_key(&self, key: &str) -> std::result::Result<(), CoreError> {
        self.handle
            .page()
            .await
            .map_err(|e| CoreError::Browser(e.to_string()))?
            .press_key(key)
            .await?;
        Ok(())
    }

    async fn click_coordinate(&self, x: f64, y: f64) -> std::result::Result<(), CoreError> {
        self.handle
            .page()
            .await
            .map_err(|e| CoreError::Browser(e.to_string()))?
            .click_coordinate(x, y)
            .await?;
        Ok(())
    }

    async fn scroll(&self, dx: i32, dy: i32) -> std::result::Result<(), CoreError> {
        self.handle
            .page()
            .await
            .map_err(|e| CoreError::Browser(e.to_string()))?
            .scroll_by(dx, dy)
            .await?;
        Ok(())
    }

    async fn current_url(&self) -> std::result::Result<Option<String>, CoreError> {
        Ok(self
            .handle
            .page()
            .await
            .map_err(|e| CoreError::Browser(e.to_string()))?
            .url()
            .await?)
    }

    async fn accessibility(&self) -> std::result::Result<AccessibilityTree, CoreError> {
        Ok(self
            .handle
            .page()
            .await
            .map_err(|e| CoreError::Browser(e.to_string()))?
            .accessibility_tree()
            .await?)
    }

    async fn screenshot_png(&self) -> std::result::Result<Vec<u8>, CoreError> {
        Ok(self
            .handle
            .page()
            .await
            .map_err(|e| CoreError::Browser(e.to_string()))?
            .screenshot_png()
            .await?)
    }
}

/// Run configuration for the spec executor.
#[derive(Debug, Clone)]
pub struct SpecRunConfig {
    /// Launch options for the browser. Defaults to headless.
    pub launch: LaunchOptions,
    /// Maximum steps to execute before aborting.
    pub max_steps: u32,
    /// Whether to record a cassette on successful runs.
    pub record_cassette: bool,
    /// Whether to attempt cassette replay first.
    pub prefer_cassette: bool,
}

impl Default for SpecRunConfig {
    fn default() -> Self {
        Self {
            launch: LaunchOptions::default(),
            max_steps: 100,
            record_cassette: false,
            prefer_cassette: false,
        }
    }
}

/// Execute an [`AtpSpec`] and return the results.
///
/// # Errors
/// Returns [`AtpSpecError::Browser`] on browser-level issues,
/// [`AtpSpecError::Assertion`] if an assertion fails.
pub async fn run_spec(
    spec: &AtpSpec,
    config: &SpecRunConfig,
    spec_path: Option<&Path>,
) -> Result<SpecResult> {
    let start = Instant::now();
    let mut steps: Vec<StepResult> = Vec::with_capacity(spec.steps.len());
    let mut passed: u32 = 0;
    let mut failed: u32 = 0;

    // 1) Launch browser.
    let handle = PageHandle::new(config.launch.clone()).await?;
    let mut browser = RunnerBrowser { handle };

    // 2) Execute steps.
    for (idx, step) in spec.steps.iter().enumerate() {
        if idx as u32 >= config.max_steps {
            steps.push(StepResult {
                seq: idx as u32,
                action: step.action_name().into(),
                success: false,
                message: Some("max steps reached".into()),
                duration_ms: 0,
                assertion_passed: None,
            });
            failed += 1;
            break;
        }

        let step_start = Instant::now();
        let result = execute_step(&browser, spec, step, idx).await;
        let duration_ms = step_start.elapsed().as_millis() as u64;

        match result {
            Ok(msg) => {
                let is_assert = step.is_assertion();
                let assert_passed = is_assert.then_some(true);
                steps.push(StepResult {
                    seq: idx as u32,
                    action: step.action_name().into(),
                    success: true,
                    message: Some(msg),
                    duration_ms,
                    assertion_passed: assert_passed,
                });
                passed += 1;
            }
            Err(err) => {
                let is_assert = step.is_assertion();
                let is_assert_fail = matches!(&err, AtpSpecError::Assertion { .. });
                let assert_passed = if is_assert {
                    Some(!is_assert_fail)
                } else {
                    None
                };

                steps.push(StepResult {
                    seq: idx as u32,
                    action: step.action_name().into(),
                    success: false,
                    message: Some(err.to_string()),
                    duration_ms,
                    assertion_passed: assert_passed,
                });
                failed += 1;

                // Assertion failures and timeouts stop execution.
                if is_assert_fail {
                    // Still record the failure but break — spec semantics.
                    break;
                }
                if !err.is_retryable() {
                    break;
                }
            }
        }
    }

    // 3) Close browser properly to avoid Chrome process leak.
    let _ = browser.handle.close().await;

    let total = steps.len() as u32;
    let duration_ms = start.elapsed().as_millis() as u64;

    Ok(SpecResult {
        name: spec.name.clone(),
        success: failed == 0,
        duration_ms,
        passed,
        failed,
        total,
        steps,
        spec_path: spec_path.map(|p| p.to_string_lossy().to_string()),
    })
}

/// Execute a single step and return a human-readable result message.
async fn execute_step(
    browser: &RunnerBrowser,
    spec: &AtpSpec,
    step: &Step,
    idx: usize,
) -> std::result::Result<String, AtpSpecError> {
    match step {
        Step::Navigate { url } => {
            let resolved = resolve_url(url, spec.base_url.as_deref());
            browser
                .navigate(&resolved)
                .await
                .map_err(|e| AtpSpecError::Browser {
                    step: idx,
                    step_name: "navigate".into(),
                    detail: format!("navigate to `{resolved}`: {e}"),
                })?;
            Ok(format!("navigated to `{resolved}`"))
        }

        Step::Click {
            selector,
            timeout_ms,
        } => {
            let backend = resolve_selector(browser, selector, idx).await?;
            let timeout = timeout_ms
                .map(|ms| Duration::from_millis(u64::from(ms)))
                .unwrap_or(DEFAULT_STEP_TIMEOUT);

            match tokio::time::timeout(timeout, browser.click_node(&backend)).await {
                Ok(Ok(())) => Ok(format!("clicked element `{backend}`")),
                Ok(Err(e)) => Err(AtpSpecError::Browser {
                    step: idx,
                    step_name: "click".into(),
                    detail: format!("click failed: {e}"),
                }),
                Err(_) => Err(AtpSpecError::Timeout {
                    elapsed: format!("{timeout:?}"),
                }),
            }
        }

        Step::Fill { selector, value } => {
            let backend = resolve_selector(browser, selector, idx).await?;
            let resolved_value = resolve_value(value, spec);
            browser
                .fill_node(&backend, &resolved_value)
                .await
                .map_err(|e| AtpSpecError::Browser {
                    step: idx,
                    step_name: "fill".into(),
                    detail: format!("fill failed: {e}"),
                })?;
            Ok(format!("filled `{}` chars", resolved_value.len()))
        }

        Step::Select { selector, value } => {
            // Select is implemented as click + press for now.
            let backend = resolve_selector(browser, selector, idx).await?;
            browser
                .click_node(&backend)
                .await
                .map_err(|e| AtpSpecError::Browser {
                    step: idx,
                    step_name: "select".into(),
                    detail: format!("select click failed: {e}"),
                })?;
            // Try pressing the value if it's short.
            if value.len() <= 1 {
                browser
                    .press_key(value)
                    .await
                    .map_err(|e| AtpSpecError::Browser {
                        step: idx,
                        step_name: "select".into(),
                        detail: format!("select key press failed: {e}"),
                    })?;
            }
            Ok(format!("selected `{value}`"))
        }

        Step::Press { key } => {
            browser
                .press_key(key)
                .await
                .map_err(|e| AtpSpecError::Browser {
                    step: idx,
                    step_name: "press".into(),
                    detail: format!("press key `{key}` failed: {e}"),
                })?;
            Ok(format!("pressed `{key}`"))
        }

        Step::Scroll { dx, dy } => {
            browser
                .scroll(*dx, *dy)
                .await
                .map_err(|e| AtpSpecError::Browser {
                    step: idx,
                    step_name: "scroll".into(),
                    detail: format!("scroll failed: {e}"),
                })?;
            Ok(format!("scrolled by ({dx}, {dy})"))
        }

        Step::Wait { ms } => {
            tokio::time::sleep(Duration::from_millis(u64::from(*ms))).await;
            Ok(format!("waited {ms} ms"))
        }

        Step::Assert { text } => {
            let page_text = extract_page_text(browser).await?;
            if page_text.to_lowercase().contains(&text.to_lowercase()) {
                Ok(format!("found `{text}`"))
            } else {
                Err(AtpSpecError::Assertion {
                    step: idx,
                    step_name: "assert".into(),
                    detail: format!("expected page to contain `{text}`, but it was not found"),
                })
            }
        }

        Step::AssertNot { text } => {
            let page_text = extract_page_text(browser).await?;
            if page_text.to_lowercase().contains(&text.to_lowercase()) {
                Err(AtpSpecError::Assertion {
                    step: idx,
                    step_name: "assert_not".into(),
                    detail: format!("expected page NOT to contain `{text}`, but it was found"),
                })
            } else {
                Ok(format!("confirmed `{text}` absent"))
            }
        }

        Step::AssertUrl { url, contains } => {
            let current = browser
                .current_url()
                .await
                .map_err(|e| AtpSpecError::Browser {
                    step: idx,
                    step_name: "assert_url".into(),
                    detail: format!("get URL failed: {e}"),
                })?
                .unwrap_or_default();

            let matched = if *contains {
                current.contains(url)
            } else {
                current == *url || current.trim_end_matches('/') == url.trim_end_matches('/')
            };

            if matched {
                Ok(format!("URL matches `{url}`: `{current}`"))
            } else {
                Err(AtpSpecError::Assertion {
                    step: idx,
                    step_name: "assert_url".into(),
                    detail: format!(
                        "expected URL to{} match `{url}`, got `{current}`",
                        if *contains { " contain" } else { "" }
                    ),
                })
            }
        }

        Step::Screenshot { name } => {
            let label = name.as_deref().unwrap_or("unnamed");
            let _ = browser
                .handle
                .page()
                .await
                .map_err(|e| AtpSpecError::Browser {
                    step: idx,
                    step_name: "screenshot".into(),
                    detail: format!("page access failed: {e}"),
                })?
                .screenshot_png()
                .await
                .map_err(|e| AtpSpecError::Browser {
                    step: idx,
                    step_name: "screenshot".into(),
                    detail: format!("screenshot failed: {e}"),
                })?;
            Ok(format!("screenshot `{label}` captured"))
        }

        Step::Extract { query, into: _into } => {
            let page_text = extract_page_text(browser).await?;
            let extracted = match query {
                Some(q) => {
                    let lower = page_text.to_lowercase();
                    let q_lower = q.to_lowercase();
                    let lines: Vec<&str> = lower.lines().filter(|l| l.contains(&q_lower)).collect();
                    lines.join("\n")
                }
                None => page_text,
            };
            if extracted.is_empty() {
                Ok("extracted 0 characters".into())
            } else {
                Ok(format!("extracted {} characters", extracted.len()))
            }
        }

        Step::Script { code } => {
            browser
                .handle
                .page()
                .await
                .map_err(|e| AtpSpecError::Browser {
                    step: idx,
                    step_name: "script".into(),
                    detail: format!("page access failed: {e}"),
                })?
                .raw()
                .evaluate(code.clone())
                .await
                .map_err(|e| AtpSpecError::Browser {
                    step: idx,
                    step_name: "script".into(),
                    detail: format!("script execution failed: {e}"),
                })?;
            Ok("script executed".into())
        }
    }
}

/// Resolve a potentially relative URL against the base URL.
fn resolve_url(url: &str, base_url: Option<&str>) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        return url.to_string();
    }
    if let Some(base) = base_url {
        let base_trimmed = base.trim_end_matches('/');
        let url_trimmed = url.trim_start_matches('/');
        format!("{base_trimmed}/{url_trimmed}")
    } else {
        url.to_string()
    }
}

/// Resolve a selector (text label or `@eN` ref) to a CDP backend node ID.
async fn resolve_selector(
    browser: &RunnerBrowser,
    selector: &ElementSelector,
    idx: usize,
) -> std::result::Result<String, AtpSpecError> {
    // If it's a direct ref, use it directly (caller must have done snapshot).
    if let Some(r) = selector.r#ref() {
        return Ok(r.to_string());
    }

    // If it's a text label, try to find it in the accessibility tree.
    if let Some(label) = selector.label() {
        let tree = browser
            .accessibility()
            .await
            .map_err(|e| AtpSpecError::Browser {
                step: idx,
                step_name: "resolve_selector".into(),
                detail: format!("failed to get accessibility tree: {e}"),
            })?;

        // Search for a node whose name matches (case-insensitive).
        let label_lower = label.to_lowercase();
        for node in tree.nodes() {
            if node.ignored {
                continue;
            }
            let matches = node
                .name
                .as_deref()
                .map(|n| n.to_lowercase().contains(&label_lower))
                .unwrap_or(false)
                || node
                    .role
                    .as_deref()
                    .map(|r| r.to_lowercase() == label_lower)
                    .unwrap_or(false);

            if matches && let Some(backend) = node.backend_dom_node_id {
                return Ok(backend.to_string());
            }
        }

        // Also search by role if no name match found.
        for node in tree.nodes() {
            if node.ignored {
                continue;
            }
            let role_matches = node
                .role
                .as_deref()
                .map(|r| r.to_lowercase() == label_lower)
                .unwrap_or(false);

            if role_matches && let Some(backend) = node.backend_dom_node_id {
                return Ok(backend.to_string());
            }
        }

        return Err(AtpSpecError::Assertion {
            step: idx,
            step_name: "resolve_selector".into(),
            detail: format!("could not find element matching `{label}` on the page"),
        });
    }

    Err(AtpSpecError::Assertion {
        step: idx,
        step_name: "resolve_selector".into(),
        detail: "element selector has neither text label nor ref".into(),
    })
}

/// Extract visible text from the current page via accessibility tree.
async fn extract_page_text(browser: &RunnerBrowser) -> std::result::Result<String, AtpSpecError> {
    let tree = browser
        .accessibility()
        .await
        .map_err(|e| AtpSpecError::Browser {
            step: 0,
            step_name: "extract".into(),
            detail: format!("failed to get accessibility tree: {e}"),
        })?;

    let mut text = String::new();
    for node in tree.nodes() {
        if node.ignored {
            continue;
        }
        let bits: Vec<&str> = [node.name.as_deref(), node.value.as_deref()]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect();

        if !bits.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&bits.join(" — "));
        }
    }
    Ok(text)
}

/// Resolve value references (e.g. `secret://...`) to actual values.
/// In interactive mode, secrets are read from env or vault.
#[must_use]
fn resolve_value(value: &str, _spec: &AtpSpec) -> String {
    if let Some(rest) = value.strip_prefix("secret://") {
        // Try environment variables first.
        let env_key = format!("GENATP_SECRET_{}", rest.replace('/', "_").to_uppercase());
        if let Ok(val) = std::env::var(&env_key) {
            return val;
        }
        // Try inline env vars from the spec.
        if let Some((_domain, key)) = rest.split_once('/')
            && let Ok(val) = std::env::var(format!("GENATP_SECRET_{}", key.to_uppercase()))
        {
            return val;
        }
        // Return the reference as-is — caller will resolve via vault.
        value.to_string()
    } else if let Some(var_name) = value.strip_prefix('$') {
        // Environment variable reference (e.g. $PRICE).
        std::env::var(var_name).unwrap_or_else(|_| value.to_string())
    } else {
        value.to_string()
    }
}

/// Compile an ATP Spec into a cassette-compatible slug.
#[must_use]
pub fn spec_to_slug(spec: &AtpSpec) -> String {
    let slug: String = spec
        .name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    slug.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir;

    #[test]
    fn resolve_absolute_url_unchanged() {
        assert_eq!(
            resolve_url("https://example.com/page", None),
            "https://example.com/page"
        );
        assert_eq!(
            resolve_url("https://example.com/page", Some("https://base.com")),
            "https://example.com/page"
        );
    }

    #[test]
    fn resolve_relative_url_with_base() {
        assert_eq!(
            resolve_url("/login", Some("https://example.com")),
            "https://example.com/login"
        );
        assert_eq!(
            resolve_url("/api/v1", Some("https://example.com/app/")),
            "https://example.com/app/api/v1"
        );
    }

    #[test]
    fn resolve_relative_url_without_base() {
        assert_eq!(resolve_url("/page", None), "/page");
    }

    #[test]
    fn spec_to_slug_works() {
        let spec = AtpSpec {
            version: ir::SCHEMA_VERSION,
            name: "Stripe Checkout! Annual Plan 2024".into(),
            description: None,
            budget: None,
            auth: vec![],
            base_url: None,
            env: vec![],
            steps: vec![],
            on_change: None,
        };
        let slug = spec_to_slug(&spec);
        assert!(!slug.contains('!'));
        assert!(slug.contains("stripe"));
        assert!(!slug.is_empty());
    }

    #[test]
    fn resolve_value_plain() {
        let spec = AtpSpec {
            version: ir::SCHEMA_VERSION,
            name: "test".into(),
            description: None,
            budget: None,
            auth: vec![],
            base_url: None,
            env: vec![],
            steps: vec![],
            on_change: None,
        };
        assert_eq!(resolve_value("hello", &spec), "hello");
        assert_eq!(resolve_value("", &spec), "");
    }

    #[test]
    fn resolve_value_secret_returns_ref_if_no_env() {
        let spec = AtpSpec {
            version: ir::SCHEMA_VERSION,
            name: "test".into(),
            description: None,
            budget: None,
            auth: vec![],
            base_url: None,
            env: vec![],
            steps: vec![],
            on_change: None,
        };
        let val = resolve_value("secret://stripe.com/sk_live", &spec);
        assert_eq!(val, "secret://stripe.com/sk_live");
    }
}
