//! Page-level CDP operations.
//!
//! Wraps a [`chromiumoxide::Page`] with the typed operations the rest of
//! genATP needs: navigate, accessibility tree, document. Returning typed
//! data (rather than CDP wire structs directly) means perception (Phase 2)
//! and orchestrator (Phase 3) don't recompile when chromiumoxide's CDP
//! types churn at the `dep` level.

use std::time::Duration;

use chromiumoxide::cdp::browser_protocol::accessibility::{
    AxNode, EnableParams as A11yEnableParams, GetFullAxTreeParams,
};
use chromiumoxide::cdp::browser_protocol::page::NavigateParams;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{BrowserError, Result};
use crate::stealth::StealthConfig;

/// Default per-operation timeout for navigation and CDP commands.
pub const DEFAULT_OP_TIMEOUT: Duration = Duration::from_secs(20);

/// Genatp-owned wrapper around a `chromiumoxide::Page`.
///
/// Cheap to clone — the underlying `Page` is `Arc`-backed.
#[derive(Clone)]
pub struct Page {
    inner: chromiumoxide::Page,
    op_timeout: Duration,
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("op_timeout", &self.op_timeout)
            .finish_non_exhaustive()
    }
}

impl Page {
    /// Wrap an existing `chromiumoxide::Page`.
    #[must_use]
    pub fn new(inner: chromiumoxide::Page) -> Self {
        Self {
            inner,
            op_timeout: DEFAULT_OP_TIMEOUT,
        }
    }

    /// Override the per-operation timeout.
    #[must_use]
    pub fn with_op_timeout(mut self, timeout: Duration) -> Self {
        self.op_timeout = timeout;
        self
    }

    /// Borrow the underlying `chromiumoxide::Page` for operations not yet
    /// wrapped here. Prefer typed wrappers when one exists.
    #[must_use]
    pub fn raw(&self) -> &chromiumoxide::Page {
        &self.inner
    }

    /// Navigate to `url`. Returns when the navigation commits or errors.
    ///
    /// # Errors
    /// - [`BrowserError::InvalidUrl`] if `url` does not parse.
    /// - [`BrowserError::Timeout`] if the underlying CDP command does not
    ///   complete within the configured op timeout.
    /// - [`BrowserError::Navigate`] if Chrome reports a navigation failure
    ///   (e.g. ERR_CONNECTION_REFUSED).
    pub async fn navigate(&self, url: &str) -> Result<()> {
        let parsed = Url::parse(url).map_err(|source| BrowserError::InvalidUrl {
            url: url.to_string(),
            source,
        })?;
        let params = NavigateParams::new(parsed.to_string());

        match tokio::time::timeout(self.op_timeout, self.inner.goto(params)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(err)) => Err(BrowserError::Navigate {
                url: url.to_string(),
                reason: err.to_string(),
            }),
            Err(_) => Err(BrowserError::Timeout(self.op_timeout)),
        }
    }

    /// Fetch the page's full accessibility tree.
    ///
    /// `Accessibility.enable` is sent first because some Chrome builds
    /// return an empty tree until accessibility instrumentation is on.
    ///
    /// # Errors
    /// Surfaces any CDP transport failure.
    pub async fn accessibility_tree(&self) -> Result<AccessibilityTree> {
        let _ = self.inner.execute(A11yEnableParams::default()).await?;

        let resp = tokio::time::timeout(
            self.op_timeout,
            self.inner.execute(GetFullAxTreeParams::default()),
        )
        .await
        .map_err(|_| BrowserError::Timeout(self.op_timeout))??;

        let nodes: Vec<AxNode> = resp.result.nodes.clone();
        Ok(AccessibilityTree::new(nodes))
    }

    /// Return the current page URL (after redirects), or `None` if Chrome
    /// reports no URL bound (rare; happens during very early load races).
    pub async fn url(&self) -> Result<Option<String>> {
        Ok(self.inner.url().await?)
    }

    /// Trigger a clean close of the underlying tab. Idempotent.
    pub async fn close(self) -> Result<()> {
        self.inner.close().await?;
        Ok(())
    }

    /// Click the DOM element identified by CDP `backendNodeId` (the value
    /// captured in [`AxNodeJson::backend_dom_node_id`]).
    ///
    /// Scrolls the node into view, then invokes `.click()` on its
    /// resolved `RemoteObject` via `Runtime.callFunctionOn`. This matches
    /// Playwright / Puppeteer's click behaviour for nodes whose visibility
    /// is already established.
    ///
    /// # Errors
    /// Returns [`BrowserError::Cdp`] on transport/CDP failures, and
    /// [`BrowserError::Timeout`] if the operation exceeds the configured
    /// op timeout.
    pub async fn click_node(&self, backend_node_id: &str) -> Result<()> {
        use chromiumoxide::cdp::browser_protocol::dom::{
            BackendNodeId, ResolveNodeParams, ScrollIntoViewIfNeededParams,
        };
        use chromiumoxide::cdp::js_protocol::runtime::CallFunctionOnParams;

        let bid_i: i64 = backend_node_id.parse().map_err(|_| {
            BrowserError::Cdp(format!("non-integer backend node id `{backend_node_id}`"))
        })?;
        let backend_id = BackendNodeId::new(bid_i);

        let _ = self
            .inner
            .execute(
                ScrollIntoViewIfNeededParams::builder()
                    .backend_node_id(backend_id)
                    .build(),
            )
            .await?;

        let resolved = self
            .inner
            .execute(
                ResolveNodeParams::builder()
                    .backend_node_id(backend_id)
                    .build(),
            )
            .await?;
        let object_id = resolved
            .result
            .object
            .object_id
            .clone()
            .ok_or_else(|| BrowserError::Cdp("resolve node returned no object id".into()))?;

        let params = CallFunctionOnParams::builder()
            .function_declaration("function() { this.click(); }")
            .object_id(object_id)
            .return_by_value(true)
            .await_promise(true)
            .build()
            .map_err(|e| BrowserError::Cdp(format!("call-fn builder: {e}")))?;

        match tokio::time::timeout(self.op_timeout, self.inner.execute(params)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(err)) => Err(BrowserError::Cdp(err.to_string())),
            Err(_) => Err(BrowserError::Timeout(self.op_timeout)),
        }
    }

    /// Fill an `<input>` / `<textarea>` element by CDP `backendNodeId`
    /// with `value`. Dispatches `input` + `change` events afterwards so
    /// JS-driven frameworks observe the change.
    ///
    /// # Errors
    /// As for [`Self::click_node`].
    pub async fn fill_node(&self, backend_node_id: &str, value: &str) -> Result<()> {
        use chromiumoxide::cdp::browser_protocol::dom::{BackendNodeId, ResolveNodeParams};
        use chromiumoxide::cdp::js_protocol::runtime::{CallArgument, CallFunctionOnParams};

        let bid_i: i64 = backend_node_id.parse().map_err(|_| {
            BrowserError::Cdp(format!("non-integer backend node id `{backend_node_id}`"))
        })?;
        let backend_id = BackendNodeId::new(bid_i);

        let resolved = self
            .inner
            .execute(
                ResolveNodeParams::builder()
                    .backend_node_id(backend_id)
                    .build(),
            )
            .await?;
        let object_id = resolved
            .result
            .object
            .object_id
            .clone()
            .ok_or_else(|| BrowserError::Cdp("resolve node returned no object id".into()))?;

        let fn_decl = "function(v) {\
            const tag = (this.tagName || '').toLowerCase();\
            if (tag === 'input' || tag === 'textarea') {\
                const proto = tag === 'input' ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;\
                const setter = Object.getOwnPropertyDescriptor(proto, 'value').set;\
                setter.call(this, v);\
            } else {\
                this.value = v;\
            }\
            this.dispatchEvent(new Event('input', { bubbles: true }));\
            this.dispatchEvent(new Event('change', { bubbles: true }));\
        }";

        let arg = CallArgument::builder()
            .value(serde_json::Value::String(value.to_string()))
            .build();
        let params = CallFunctionOnParams::builder()
            .function_declaration(fn_decl)
            .object_id(object_id)
            .return_by_value(true)
            .await_promise(true)
            .argument(arg)
            .build()
            .map_err(|e| BrowserError::Cdp(format!("fill builder: {e}")))?;
        match tokio::time::timeout(self.op_timeout, self.inner.execute(params)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(err)) => Err(BrowserError::Cdp(err.to_string())),
            Err(_) => Err(BrowserError::Timeout(self.op_timeout)),
        }
    }

    /// Send a single keystroke (e.g. `"Enter"`, `"Tab"`) to the page. The
    /// key is delivered as a `keyDown` immediately followed by a `keyUp`.
    ///
    /// # Errors
    /// As for [`Self::click_node`].
    pub async fn press_key(&self, key: &str) -> Result<()> {
        use chromiumoxide::cdp::browser_protocol::input::{
            DispatchKeyEventParams, DispatchKeyEventType,
        };
        for kind in [DispatchKeyEventType::KeyDown, DispatchKeyEventType::KeyUp] {
            let params = DispatchKeyEventParams::builder()
                .r#type(kind)
                .key(key)
                .build()
                .map_err(|e| BrowserError::Cdp(format!("key event builder: {e}")))?;
            match tokio::time::timeout(self.op_timeout, self.inner.execute(params)).await {
                Ok(Ok(_)) => {}
                Ok(Err(err)) => return Err(BrowserError::Cdp(err.to_string())),
                Err(_) => return Err(BrowserError::Timeout(self.op_timeout)),
            }
        }
        Ok(())
    }

    /// Inject stealth fingerprint override scripts into the current page.
    /// Uses `Runtime.evaluate` to inject the script on the current blank
    /// page (`about:blank`) **before** the target URL is navigated to.
    /// This ensures overrides take effect before any page-level JS runs.
    ///
    /// **Note:** `Runtime.evaluate` is used rather than
    /// `Page.addScriptToEvaluateOnNewDocument` because the latter is not
    /// directly exposed by the chromiumoxide version we depend on. The
    /// practical effect is the same when called on the blank `about:blank`
    /// page before navigation — the overrides persist through the subsequent
    /// same-origin navigation. Cross-origin navigations and dynamically
    /// created iframes are not covered.
    ///
    /// # Errors
    /// Returns [`BrowserError::Cdp`] if the CDP command fails.
    pub async fn inject_stealth(&self, script: &str) -> Result<()> {
        use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;

        let params = EvaluateParams::builder()
            .expression(script)
            .return_by_value(true)
            .build()
            .map_err(|e| BrowserError::Cdp(format!("stealth script builder: {e}")))?;

        match tokio::time::timeout(self.op_timeout, self.inner.execute(params)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(err)) => Err(BrowserError::Cdp(err.to_string())),
            Err(_) => Err(BrowserError::Timeout(self.op_timeout)),
        }
    }

    /// Apply stealth configuration to this page: inject fingerprint
    /// override scripts and set CDP emulation overrides for timezone
    /// if configured.
    ///
    /// Uses a single [`FingerprintProfile`] for both the JS overrides
    /// and the CDP emulation domain, ensuring consistency.
    ///
    /// # Errors
    /// Returns [`BrowserError::Cdp`] if any CDP command fails.
    pub async fn apply_stealth(&self, config: &mut StealthConfig) -> Result<()> {
        if !config.enabled {
            return Ok(());
        }

        // Copy configuration fields before mutably borrowing config for
        // next_profile(), to satisfy Rust's borrow checker.
        let extra = config.extra;
        let use_cdp = config.use_cdp_emulation;

        // Get the next profile once — used for both JS overrides and
        // CDP emulation, ensuring the timezone/locale/webgl fields are
        // consistent across both injection paths.
        let profile = match config.next_profile() {
            Some(p) => p,
            None => return Ok(()),
        };

        // Inject the JS override script.
        let script = crate::stealth::build_stealth_script(profile, &extra);
        self.inject_stealth(&script).await?;

        // Apply CDP emulation overrides if enabled.
        if use_cdp {
            self.apply_cdp_emulation(profile).await?;
        }

        Ok(())
    }

    /// Set CDP emulation overrides (timezone, locale, etc.) on the page.
    /// Currently overrides `Intl.DateTimeFormat` to return the profile's
    /// timezone. This is done via `Runtime.evaluate` rather than the raw
    /// CDP `Emulation.setTimezoneOverride` command, which may not be
    /// available in all chromiumoxide versions.
    async fn apply_cdp_emulation(
        &self,
        profile: &crate::stealth::FingerprintProfile,
    ) -> Result<()> {
        use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;

        let script = format!(
            r#"
(function() {{
    try {{
        const tz = "{timezone}";
        const OrigDateTimeFormat = Intl.DateTimeFormat;
        Intl.DateTimeFormat = function(locales, options) {{
            const opts = Object.assign({{}}, options, {{ timeZone: tz }});
            return new OrigDateTimeFormat(locales, opts);
        }};
        Intl.DateTimeFormat.prototype = OrigDateTimeFormat.prototype;
    }} catch(e) {{}}
}})();
"#,
            timezone = profile.timezone
        );

        let params = EvaluateParams::builder()
            .expression(&script)
            .return_by_value(true)
            .build()
            .map_err(|e| BrowserError::Cdp(format!("stealth emulation builder: {e}")))?;

        match tokio::time::timeout(self.op_timeout, self.inner.execute(params)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(err)) => {
                tracing::warn!(
                    target = "genatp::stealth",
                    "CDP emulation timezone override failed: {err}"
                );
                Ok(())
            }
            Err(_) => {
                tracing::warn!(target = "genatp::stealth", "CDP emulation timeout");
                Ok(())
            }
        }
    }

    /// Capture a full-viewport PNG screenshot. Returns the raw PNG bytes;
    /// callers wrap in base64 / save to disk as they see fit.
    ///
    /// # Errors
    /// As for [`Self::click_node`].
    pub async fn screenshot_png(&self) -> Result<Vec<u8>> {
        use base64::Engine as _;
        use chromiumoxide::cdp::browser_protocol::page::{
            CaptureScreenshotFormat, CaptureScreenshotParams,
        };

        let params = CaptureScreenshotParams::builder()
            .format(CaptureScreenshotFormat::Png)
            .build();
        let resp = tokio::time::timeout(self.op_timeout, self.inner.execute(params))
            .await
            .map_err(|_| BrowserError::Timeout(self.op_timeout))??;
        // CDP returns the screenshot as a base64 string (chromiumoxide
        // exposes it via the `Binary` newtype around `String`).
        let data: &str = resp.result.data.as_ref();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| BrowserError::Cdp(format!("screenshot base64 decode: {e}")))?;
        Ok(bytes)
    }

    /// Navigate back in the session history. Returns `true` if the
    /// browser actually moved; `false` if there was nothing to go back
    /// to.
    ///
    /// # Errors
    /// As for [`Self::click_node`].
    pub async fn go_back(&self) -> Result<bool> {
        use chromiumoxide::cdp::browser_protocol::page::{
            GetNavigationHistoryParams, NavigateToHistoryEntryParams,
        };

        let history = self
            .inner
            .execute(GetNavigationHistoryParams::default())
            .await?;
        let current_index = history.result.current_index;
        if current_index <= 0 {
            return Ok(false);
        }
        let target = &history.result.entries[(current_index - 1) as usize];
        self.inner
            .execute(NavigateToHistoryEntryParams::new(target.id))
            .await?;
        Ok(true)
    }

    /// Scroll the viewport by relative pixels via `window.scrollBy`.
    ///
    /// # Errors
    /// As for [`Self::click_node`].
    pub async fn scroll_by(&self, dx: i32, dy: i32) -> Result<()> {
        use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;
        let params = EvaluateParams::builder()
            .expression(format!("window.scrollBy({dx}, {dy})"))
            .return_by_value(true)
            .build()
            .map_err(|e| BrowserError::Cdp(format!("scroll builder: {e}")))?;
        match tokio::time::timeout(self.op_timeout, self.inner.execute(params)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(err)) => Err(BrowserError::Cdp(err.to_string())),
            Err(_) => Err(BrowserError::Timeout(self.op_timeout)),
        }
    }
}

/// Owned, simplified projection of a Chrome accessibility tree.
///
/// Phase 1 ships the raw node list and helper accessors. Phase 2 (perception)
/// will compress, ref-tag (`@e1`, `@e2`), and diff this structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityTree {
    nodes: Vec<AxNodeJson>,
}

impl AccessibilityTree {
    fn new(nodes: Vec<AxNode>) -> Self {
        Self {
            nodes: nodes.into_iter().map(AxNodeJson::from_cdp).collect(),
        }
    }

    /// Build a tree from already-projected nodes. Used by `genatp-core`
    /// tests and the perception bridge to construct fixtures without
    /// touching CDP wire types.
    #[must_use]
    pub fn from_nodes(nodes: Vec<AxNodeJson>) -> Self {
        Self { nodes }
    }

    /// Total number of accessibility nodes in the tree.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// `true` if the tree contains no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Borrow the node list.
    #[must_use]
    pub fn nodes(&self) -> &[AxNodeJson] {
        &self.nodes
    }

    /// Count nodes whose role indicates user-interactivity. Used by the
    /// Phase 1 acceptance test ("≥ 1 interactive element on hello.html").
    #[must_use]
    pub fn interactive_count(&self) -> usize {
        self.nodes.iter().filter(|n| n.is_interactive()).count()
    }
}

/// Lightweight, JSON-friendly mirror of `chromiumoxide`'s `AxNode`.
///
/// We don't expose the upstream type directly to keep its churn isolated
/// from perception/orchestrator consumers. Phase 3 expanded this struct to
/// preserve parent/child relationships, current `value`, boolean ARIA
/// properties, and the underlying DOM `backendNodeId` — the perception
/// engine ([`genatp-perception`]) needs the first set, the orchestrator
/// needs the last one to dispatch DOM-level actions like `.click()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxNodeJson {
    /// Accessibility node ID (Chrome's `Accessibility.AXNodeId`). Different
    /// namespace from `DOM.NodeId` / `DOM.BackendNodeId` — only use it as
    /// an identifier within this tree.
    pub node_id: String,
    /// Parent node id, or `None` for the root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Ordered list of child ids (document order).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub child_ids: Vec<String>,
    /// ARIA / HTML role (e.g. `"button"`, `"link"`, `"text"`).
    pub role: Option<String>,
    /// Accessible name (button label, link text, …).
    pub name: Option<String>,
    /// Current value (input value, slider position, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Accessible description (often `aria-describedby`).
    pub description: Option<String>,
    /// Boolean ARIA properties (focused / selected / disabled / …). Each
    /// property is encoded as `(name, value)` so this stays portable to JSON.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<AxPropertyJson>,
    /// CDP `backendNodeId` of the underlying DOM node (when one exists).
    /// `None` for synthetic nodes (`WebArea`, etc). Used by the
    /// orchestrator's action dispatcher to drive clicks / fills.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_dom_node_id: Option<i64>,
    /// Whether the node is currently in the a11y tree (chrome ignores some).
    pub ignored: bool,
}

/// One ARIA property pair captured from CDP. Property names are lower-cased
/// to match the convention used elsewhere in the perception pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AxPropertyJson {
    /// Property name (e.g. `"focused"`, `"selected"`, `"disabled"`).
    pub name: String,
    /// Property value as a string (`"true"`, `"false"`, `"mixed"`, …).
    pub value: String,
}

impl AxNodeJson {
    fn from_cdp(node: AxNode) -> Self {
        let extract_str = |opt: Option<
            chromiumoxide::cdp::browser_protocol::accessibility::AxValue,
        >|
         -> Option<String> {
            opt.and_then(|v| v.value).map(|v| match v {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            })
        };

        let property_value =
            |v: chromiumoxide::cdp::browser_protocol::accessibility::AxValue| -> String {
                v.value
                    .map(|raw| match raw {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    })
                    .unwrap_or_default()
            };

        let properties = node
            .properties
            .unwrap_or_default()
            .into_iter()
            .map(|p| AxPropertyJson {
                name: format!("{:?}", p.name).to_lowercase(),
                value: property_value(p.value),
            })
            .collect();

        let child_ids = node
            .child_ids
            .unwrap_or_default()
            .into_iter()
            .map(|id| id.inner().clone())
            .collect();

        Self {
            node_id: node.node_id.inner().clone(),
            parent_id: node.parent_id.map(|id| id.inner().clone()),
            child_ids,
            role: extract_str(node.role),
            name: extract_str(node.name),
            value: extract_str(node.value),
            description: extract_str(node.description),
            properties,
            backend_dom_node_id: node.backend_dom_node_id.map(|b| *b.inner()),
            ignored: node.ignored,
        }
    }

    /// True if this node represents something a user can act on (button,
    /// link, input, etc.). Heuristic — refined in Phase 2.
    #[must_use]
    pub fn is_interactive(&self) -> bool {
        const INTERACTIVE_ROLES: &[&str] = &[
            "button",
            "link",
            "textbox",
            "checkbox",
            "radio",
            "combobox",
            "menuitem",
            "tab",
            "switch",
            "searchbox",
        ];
        match self.role.as_deref() {
            Some(role) => INTERACTIVE_ROLES.contains(&role),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tree(nodes: Vec<(&str, &str)>) -> AccessibilityTree {
        AccessibilityTree {
            nodes: nodes
                .into_iter()
                .map(|(role, name)| AxNodeJson {
                    node_id: "1".into(),
                    parent_id: None,
                    child_ids: vec![],
                    role: Some(role.into()),
                    name: Some(name.into()),
                    value: None,
                    description: None,
                    properties: vec![],
                    backend_dom_node_id: None,
                    ignored: false,
                })
                .collect(),
        }
    }

    #[test]
    fn interactive_count_recognises_button_and_link() {
        let t = make_tree(vec![
            ("button", "Submit"),
            ("link", "Home"),
            ("text", "Hello"),
        ]);
        assert_eq!(t.interactive_count(), 2);
    }

    #[test]
    fn unknown_role_is_not_interactive() {
        let n = AxNodeJson {
            node_id: "1".into(),
            parent_id: None,
            child_ids: vec![],
            role: Some("paragraph".into()),
            name: None,
            value: None,
            description: None,
            properties: vec![],
            backend_dom_node_id: None,
            ignored: false,
        };
        assert!(!n.is_interactive());
    }

    #[test]
    fn missing_role_is_not_interactive() {
        let n = AxNodeJson {
            node_id: "1".into(),
            parent_id: None,
            child_ids: vec![],
            role: None,
            name: None,
            value: None,
            description: None,
            properties: vec![],
            backend_dom_node_id: None,
            ignored: false,
        };
        assert!(!n.is_interactive());
    }

    #[test]
    fn empty_tree_reports_len_zero() {
        let t = AccessibilityTree { nodes: vec![] };
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
        assert_eq!(t.interactive_count(), 0);
    }

    #[test]
    fn tree_serialises_to_json() {
        let t = make_tree(vec![("button", "Go")]);
        let json = serde_json::to_string(&t).expect("serializes");
        assert!(json.contains("button"));
        assert!(json.contains("Go"));
    }
}
