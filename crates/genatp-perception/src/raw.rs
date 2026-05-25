//! Raw a11y tree input — the perception engine's contract with the browser
//! crate (and any other CDP frontend).
//!
//! The browser substrate (Phase 1) emits CDP `AXNode` records. Rather than
//! depend on `genatp-browser` (and indirectly on `chromiumoxide`), perception
//! defines its own owned, JSON-friendly mirror of the bits we actually use.
//! Conversion lives in the orchestrator (Phase 3), where browser → perception
//! coupling is appropriate.
//!
//! This is also the unit that fixture corpora and integration tests build by
//! hand, so the type intentionally mirrors the **post-CDP, pre-perception**
//! shape rather than CDP wire types directly.

use serde::{Deserialize, Serialize};

/// One accessibility node, identified by its CDP node id (a string in
/// chromiumoxide because CDP returns it as a string-quoted integer).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RawNode {
    /// Stable per-tree node id. Unique within the tree.
    pub node_id: String,

    /// Parent id, or `None` for the root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,

    /// Ordered list of child ids (significant order: defines tab order
    /// + visual layout for refs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub child_ids: Vec<String>,

    /// ARIA / HTML role (e.g. `"button"`, `"link"`). `None` is rare and
    /// signals an internal, ignored node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,

    /// Accessible name (`aria-label`, button text, link text…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Current value (`<input>` value, slider position…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    /// Accessible description (`aria-describedby`, tooltip…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Boolean / enumerated ARIA properties (focused, selected, disabled…).
    /// Exposed as a flat key/value list — perception cares only about a small
    /// fixed set (see [`crate::snapshot::NodeFlags`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<RawProperty>,

    /// Chrome-side ignored flag (the engine still emits the node but marks
    /// it irrelevant to assistive tech).
    #[serde(default)]
    pub ignored: bool,
}

/// One ARIA property name → string value pair.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RawProperty {
    pub name: String,
    pub value: String,
}

impl RawProperty {
    /// Convenience constructor.
    #[must_use]
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// A complete raw a11y tree as captured at one instant.
///
/// `url` is the post-redirect document URL (perception uses it for the
/// snapshot's identity, never as a navigational primitive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawTree {
    pub url: String,
    /// Capture timestamp (Unix milliseconds). `0` is acceptable in tests.
    #[serde(default)]
    pub captured_at_ms: u64,
    pub nodes: Vec<RawNode>,
}

impl RawTree {
    /// Build a tree with a fixed timestamp — convenient for fixtures.
    #[must_use]
    pub fn new(url: impl Into<String>, nodes: Vec<RawNode>) -> Self {
        Self {
            url: url.into(),
            captured_at_ms: 0,
            nodes,
        }
    }

    /// Resolve the root node id by finding the unique node without a parent.
    /// Returns `None` if there is no root (empty tree) or multiple roots
    /// (perception treats the first parent-less node as root in that case;
    /// callers can use this method to validate before compressing).
    #[must_use]
    pub fn root_id(&self) -> Option<&str> {
        self.nodes
            .iter()
            .find(|n| n.parent_id.is_none())
            .map(|n| n.node_id.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tree_has_no_root() {
        let t = RawTree::new("about:blank", vec![]);
        assert!(t.root_id().is_none());
    }

    #[test]
    fn root_id_returns_unique_parentless_node() {
        let nodes = vec![
            RawNode {
                node_id: "1".into(),
                parent_id: None,
                child_ids: vec!["2".into()],
                role: Some("WebArea".into()),
                ..default()
            },
            RawNode {
                node_id: "2".into(),
                parent_id: Some("1".into()),
                child_ids: vec![],
                role: Some("button".into()),
                name: Some("OK".into()),
                ..default()
            },
        ];
        let t = RawTree::new("https://x.test", nodes);
        assert_eq!(t.root_id(), Some("1"));
    }

    fn default() -> RawNode {
        RawNode {
            node_id: String::new(),
            parent_id: None,
            child_ids: vec![],
            role: None,
            name: None,
            value: None,
            description: None,
            properties: vec![],
            ignored: false,
        }
    }

    #[test]
    fn raw_node_round_trips_through_serde() {
        let n = RawNode {
            node_id: "42".into(),
            parent_id: Some("1".into()),
            child_ids: vec!["43".into()],
            role: Some("button".into()),
            name: Some("Submit".into()),
            value: Some("yes".into()),
            description: None,
            properties: vec![RawProperty::new("focused", "true")],
            ignored: false,
        };
        let json = serde_json::to_string(&n).unwrap();
        let back: RawNode = serde_json::from_str(&json).unwrap();
        assert_eq!(n, back);
    }
}
