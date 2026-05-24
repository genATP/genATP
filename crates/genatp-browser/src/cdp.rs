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
/// from perception/orchestrator consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxNodeJson {
    /// Stable per-snapshot node ID (matches Chrome's `nodeId`).
    pub node_id: String,
    /// ARIA / HTML role (e.g. `"button"`, `"link"`, `"text"`).
    pub role: Option<String>,
    /// Accessible name (button label, link text, …).
    pub name: Option<String>,
    /// Accessible description (often `aria-describedby`).
    pub description: Option<String>,
    /// Whether the node is currently in the a11y tree (chrome ignores some).
    pub ignored: bool,
}

impl AxNodeJson {
    fn from_cdp(node: AxNode) -> Self {
        let extract =
            |opt: Option<chromiumoxide::cdp::browser_protocol::accessibility::AxValue>| {
                opt.and_then(|v| v.value)
                    .and_then(|v| v.as_str().map(String::from))
            };

        Self {
            node_id: node.node_id.inner().clone(),
            role: extract(node.role),
            name: extract(node.name),
            description: extract(node.description),
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
                    role: Some(role.into()),
                    name: Some(name.into()),
                    description: None,
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
            role: Some("paragraph".into()),
            name: None,
            description: None,
            ignored: false,
        };
        assert!(!n.is_interactive());
    }

    #[test]
    fn missing_role_is_not_interactive() {
        let n = AxNodeJson {
            node_id: "1".into(),
            role: None,
            name: None,
            description: None,
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
