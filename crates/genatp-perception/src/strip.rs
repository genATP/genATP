//! ARIA noise stripper.
//!
//! Chrome emits an a11y tree that is faithful but verbose. The agent does
//! not need decorative wrappers (`generic`, `none`, `presentation`), empty
//! groupings, or whitespace-only text nodes. Stripping these before ref
//! compression cuts the token bill ~5x on a typical landing page without
//! losing actionable structure.
//!
//! The stripper is **structural**: it removes a node and re-parents that
//! node's children to its parent so the tree shape stays connected. This
//! preserves stacking order without leaking decorative semantics.
//!
//! Roles preserved by default:
//! - all interactive roles (button, link, textbox, …)
//! - all heading / landmark roles (heading, banner, navigation, …)
//! - `WebArea` / `RootWebArea` (the document)
//! - text leaves with non-empty names
//!
//! Roles removed by default: `generic`, `none`, `presentation`, `inline`,
//! `LineBreak`, `paragraph` *only when empty*, plus any node Chrome marks
//! `ignored = true`.

use std::collections::{HashMap, HashSet};

use crate::raw::{RawNode, RawTree};

/// Configurable stripping policy. Default is the production policy.
#[derive(Debug, Clone)]
pub struct StripPolicy {
    /// Roles that are always dropped (children re-parented).
    pub decorative_roles: HashSet<String>,
    /// Roles dropped only when their accessible name is empty.
    pub empty_dropped_roles: HashSet<String>,
    /// If `true`, also drop `ignored = true` nodes.
    pub drop_ignored: bool,
    /// If `true`, drop nodes with role `text` that have empty / whitespace name.
    pub drop_empty_text: bool,
}

impl Default for StripPolicy {
    fn default() -> Self {
        let decorative = [
            "generic",
            "none",
            "presentation",
            "InlineTextBox",
            "LineBreak",
            "Inline",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();

        let empty_dropped = ["paragraph", "group", "Section", "GenericContainer"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();

        Self {
            decorative_roles: decorative,
            empty_dropped_roles: empty_dropped,
            drop_ignored: true,
            drop_empty_text: true,
        }
    }
}

impl StripPolicy {
    /// Disable all stripping — useful for perception-engine tests that need
    /// the raw tree shape preserved.
    #[must_use]
    pub fn passthrough() -> Self {
        Self {
            decorative_roles: HashSet::new(),
            empty_dropped_roles: HashSet::new(),
            drop_ignored: false,
            drop_empty_text: false,
        }
    }
}

/// Run the stripper. Returns a new tree with decorative noise removed and
/// children re-parented to keep the tree connected.
#[must_use]
pub fn strip(tree: &RawTree, policy: &StripPolicy) -> RawTree {
    let by_id: HashMap<&str, &RawNode> =
        tree.nodes.iter().map(|n| (n.node_id.as_str(), n)).collect();

    let drop_ids: HashSet<String> = tree
        .nodes
        .iter()
        .filter(|n| should_drop(n, policy))
        .map(|n| n.node_id.clone())
        .collect();

    let mut out: Vec<RawNode> = Vec::with_capacity(tree.nodes.len() - drop_ids.len());

    for n in &tree.nodes {
        if drop_ids.contains(&n.node_id) {
            continue;
        }

        let new_parent = effective_parent(n.parent_id.as_deref(), &drop_ids, &by_id);
        let new_children = expand_children(&n.child_ids, &drop_ids, &by_id);

        out.push(RawNode {
            node_id: n.node_id.clone(),
            parent_id: new_parent,
            child_ids: new_children,
            role: n.role.clone(),
            name: n.name.clone(),
            value: n.value.clone(),
            description: n.description.clone(),
            properties: n.properties.clone(),
            ignored: n.ignored,
        });
    }

    RawTree {
        url: tree.url.clone(),
        captured_at_ms: tree.captured_at_ms,
        nodes: out,
    }
}

fn should_drop(n: &RawNode, policy: &StripPolicy) -> bool {
    if policy.drop_ignored && n.ignored {
        return true;
    }
    let Some(role) = n.role.as_deref() else {
        return true;
    };
    if policy.decorative_roles.contains(role) {
        return true;
    }
    if policy.empty_dropped_roles.contains(role) {
        let empty_name = n.name.as_deref().is_none_or(|s| s.trim().is_empty());
        if empty_name {
            return true;
        }
    }
    if policy.drop_empty_text
        && (role == "text" || role == "StaticText")
        && n.name.as_deref().is_none_or(|s| s.trim().is_empty())
    {
        return true;
    }
    false
}

/// Resolve a node's effective parent after stripping: walk up until we hit
/// a parent that is not being dropped (or the root).
fn effective_parent<'a>(
    mut parent: Option<&'a str>,
    drop: &HashSet<String>,
    by_id: &HashMap<&'a str, &'a RawNode>,
) -> Option<String> {
    while let Some(p) = parent {
        if !drop.contains(p) {
            return Some(p.to_string());
        }
        parent = by_id.get(p).and_then(|n| n.parent_id.as_deref());
    }
    None
}

/// Resolve a node's effective children after stripping. Each dropped child
/// is replaced by *its* effective children (recursively), in the original
/// document order.
fn expand_children(
    ids: &[String],
    drop: &HashSet<String>,
    by_id: &HashMap<&str, &RawNode>,
) -> Vec<String> {
    let mut out = Vec::with_capacity(ids.len());
    let mut stack: Vec<&str> = ids.iter().rev().map(String::as_str).collect();
    while let Some(id) = stack.pop() {
        if drop.contains(id) {
            if let Some(node) = by_id.get(id) {
                for c in node.child_ids.iter().rev() {
                    stack.push(c.as_str());
                }
            }
        } else {
            out.push(id.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::RawNode;

    fn node(id: &str, parent: Option<&str>, children: &[&str], role: &str, name: &str) -> RawNode {
        RawNode {
            node_id: id.into(),
            parent_id: parent.map(str::to_string),
            child_ids: children.iter().map(|s| (*s).to_string()).collect(),
            role: Some(role.into()),
            name: if name.is_empty() {
                None
            } else {
                Some(name.into())
            },
            value: None,
            description: None,
            properties: vec![],
            ignored: false,
        }
    }

    #[test]
    fn drops_generic_and_reparents_children() {
        let tree = RawTree::new(
            "x",
            vec![
                node("1", None, &["2"], "WebArea", ""),
                node("2", Some("1"), &["3"], "generic", ""),
                node("3", Some("2"), &[], "button", "OK"),
            ],
        );
        let stripped = strip(&tree, &StripPolicy::default());
        let ids: Vec<&str> = stripped.nodes.iter().map(|n| n.node_id.as_str()).collect();
        assert_eq!(ids, vec!["1", "3"]);
        let button = stripped.nodes.iter().find(|n| n.node_id == "3").unwrap();
        assert_eq!(button.parent_id.as_deref(), Some("1"));
        let root = stripped.nodes.iter().find(|n| n.node_id == "1").unwrap();
        assert_eq!(root.child_ids, vec!["3".to_string()]);
    }

    #[test]
    fn drops_ignored_nodes() {
        let mut hidden = node("2", Some("1"), &[], "button", "Hidden");
        hidden.ignored = true;
        let tree = RawTree::new("x", vec![node("1", None, &["2"], "WebArea", ""), hidden]);
        let stripped = strip(&tree, &StripPolicy::default());
        assert_eq!(stripped.nodes.len(), 1);
    }

    #[test]
    fn keeps_named_paragraph_drops_empty_paragraph() {
        let tree = RawTree::new(
            "x",
            vec![
                node("1", None, &["2", "3"], "WebArea", ""),
                node("2", Some("1"), &[], "paragraph", "Hello world."),
                node("3", Some("1"), &[], "paragraph", ""),
            ],
        );
        let stripped = strip(&tree, &StripPolicy::default());
        let kept_ids: Vec<&str> = stripped.nodes.iter().map(|n| n.node_id.as_str()).collect();
        assert_eq!(kept_ids, vec!["1", "2"]);
    }

    #[test]
    fn passthrough_strips_nothing() {
        let tree = RawTree::new(
            "x",
            vec![
                node("1", None, &["2"], "WebArea", ""),
                node("2", Some("1"), &[], "generic", ""),
            ],
        );
        let stripped = strip(&tree, &StripPolicy::passthrough());
        assert_eq!(stripped.nodes.len(), 2);
    }

    #[test]
    fn nested_decorative_chains_collapse() {
        let tree = RawTree::new(
            "x",
            vec![
                node("1", None, &["2"], "WebArea", ""),
                node("2", Some("1"), &["3"], "generic", ""),
                node("3", Some("2"), &["4"], "presentation", ""),
                node("4", Some("3"), &[], "button", "Click"),
            ],
        );
        let stripped = strip(&tree, &StripPolicy::default());
        let ids: Vec<&str> = stripped.nodes.iter().map(|n| n.node_id.as_str()).collect();
        assert_eq!(ids, vec!["1", "4"]);
        let btn = stripped.nodes.iter().find(|n| n.node_id == "4").unwrap();
        assert_eq!(btn.parent_id.as_deref(), Some("1"));
        let root = stripped.nodes.iter().find(|n| n.node_id == "1").unwrap();
        assert_eq!(root.child_ids, vec!["4".to_string()]);
    }
}
