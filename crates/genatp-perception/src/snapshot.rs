//! Compressed snapshot model — what perception emits and what the LLM sees.
//!
//! The compressed snapshot is the **canonical, token-efficient** representation
//! of a page state. It's a flat list of [`CompressedNode`]s in pre-order
//! traversal, each carrying a stable [`Ref`] (`@e1`, `@e2`, …) and a parent
//! ref + position.
//!
//! Why flat (not nested)?
//! - Diffs become per-ref add/remove/replace (see [`crate::diff`]).
//! - Pre-order traversal is cheap to recover for rendering.
//! - JSON serialisation stays compact: one `{ ... }` per node, no nesting tax.
//!
//! Canonicalisation: nodes are always sorted in pre-order DFS from the root.
//! This makes byte-equality on the serialised form a meaningful test of
//! semantic equality.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// Stable, human-readable element reference. Strings of the form `"@e1"`,
/// `"@e2"`, … assigned by the [`crate::refs::RefMap`].
pub type Ref = String;

/// Boolean ARIA state of a node, bit-packed for efficient serialisation
/// and so that diff updates can express *which* flags changed cheaply.
///
/// Only flags that demonstrably affect agent decision-making live here.
/// Fingerprint-style noise (`describedby`, `controls`, …) is dropped during
/// compression — see [`crate::strip`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "Vec<&'static str>", from = "Vec<String>")]
pub struct NodeFlags {
    pub focused: bool,
    pub selected: bool,
    pub checked: bool,
    pub expanded: bool,
    pub disabled: bool,
    pub required: bool,
    pub invalid: bool,
    pub readonly: bool,
}

impl NodeFlags {
    /// Iterate the names of the flags that are set, in stable order.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.focused {
            out.push("focused");
        }
        if self.selected {
            out.push("selected");
        }
        if self.checked {
            out.push("checked");
        }
        if self.expanded {
            out.push("expanded");
        }
        if self.disabled {
            out.push("disabled");
        }
        if self.required {
            out.push("required");
        }
        if self.invalid {
            out.push("invalid");
        }
        if self.readonly {
            out.push("readonly");
        }
        out
    }

    /// Parse from the flat string list produced by serialisation.
    #[must_use]
    pub fn from_names<S: AsRef<str>>(names: &[S]) -> Self {
        let mut f = Self::default();
        for n in names {
            match n.as_ref() {
                "focused" => f.focused = true,
                "selected" => f.selected = true,
                "checked" => f.checked = true,
                "expanded" => f.expanded = true,
                "disabled" => f.disabled = true,
                "required" => f.required = true,
                "invalid" => f.invalid = true,
                "readonly" => f.readonly = true,
                _ => {}
            }
        }
        f
    }

    /// `true` if no flag is set. Useful for skipping the field on serialise.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !(self.focused
            || self.selected
            || self.checked
            || self.expanded
            || self.disabled
            || self.required
            || self.invalid
            || self.readonly)
    }
}

impl From<NodeFlags> for Vec<&'static str> {
    fn from(f: NodeFlags) -> Self {
        f.names()
    }
}

impl From<Vec<String>> for NodeFlags {
    fn from(v: Vec<String>) -> Self {
        Self::from_names(&v)
    }
}

/// One node in a compressed snapshot.
///
/// Field order matters for canonical JSON serialisation — keep it stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompressedNode {
    /// `@eN` element reference. Stable across snapshots within a session.
    #[serde(rename = "ref")]
    pub r#ref: Ref,

    /// ARIA / HTML role. Always present after compression — nodes with no
    /// role are stripped out.
    pub role: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    /// Parent ref. Empty string for the root.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub parent: Ref,

    /// Position among siblings under `parent` (0-based).
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub position: usize,

    #[serde(default, skip_serializing_if = "NodeFlags::is_empty")]
    pub flags: NodeFlags,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde signature constraint
fn is_zero_usize(n: &usize) -> bool {
    *n == 0
}

/// A complete compressed snapshot: pre-order DFS list of nodes plus identity
/// metadata (`url`, `captured_at_ms`).
///
/// **Canonicalisation invariant:** [`Self::canonicalize`] orders `nodes` such
/// that the root node comes first and each child appears after its parent in
/// `position` order. Two snapshots that are semantically equal produce
/// byte-identical JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompressedSnapshot {
    pub url: String,
    #[serde(default)]
    pub captured_at_ms: u64,
    /// Root ref (`""` if the snapshot has no nodes; otherwise the parent
    /// of every root-level node).
    #[serde(default)]
    pub root: Ref,
    pub nodes: Vec<CompressedNode>,
}

impl CompressedSnapshot {
    /// Empty snapshot — useful for "first frame" diffs.
    #[must_use]
    pub fn empty(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            captured_at_ms: 0,
            root: String::new(),
            nodes: Vec::new(),
        }
    }

    /// Number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// `true` if no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Look up a node by ref. `O(n)` — for hot paths build a map.
    #[must_use]
    pub fn find(&self, r#ref: &str) -> Option<&CompressedNode> {
        self.nodes.iter().find(|n| n.r#ref == r#ref)
    }

    /// Build a `Ref → &CompressedNode` map.
    #[must_use]
    pub fn index(&self) -> HashMap<&str, &CompressedNode> {
        self.nodes.iter().map(|n| (n.r#ref.as_str(), n)).collect()
    }

    /// Sort nodes into pre-order DFS canonical order. Idempotent.
    ///
    /// After calling this, byte-equality on serialised JSON is a sound
    /// equality check for two snapshots.
    ///
    /// Defensive against duplicate refs (in which case the first occurrence
    /// wins) and disconnected nodes (appended in stable input order after
    /// the connected forest).
    pub fn canonicalize(&mut self) {
        let mut by_parent: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, node) in self.nodes.iter().enumerate() {
            by_parent.entry(node.parent.clone()).or_default().push(idx);
        }
        for siblings in by_parent.values_mut() {
            siblings.sort_by(|&a, &b| {
                let na = &self.nodes[a];
                let nb = &self.nodes[b];
                na.position
                    .cmp(&nb.position)
                    .then_with(|| na.r#ref.cmp(&nb.r#ref))
            });
        }

        let root_idx = self
            .nodes
            .iter()
            .position(|n| n.r#ref == self.root && n.parent.is_empty());

        let mut new_order: Vec<usize> = Vec::with_capacity(self.nodes.len());
        let mut visited_idx: HashSet<usize> = HashSet::new();
        let mut visited_ref: HashSet<String> = HashSet::new();
        let mut stack: Vec<usize> = Vec::new();

        if let Some(idx) = root_idx {
            stack.push(idx);
        } else if let Some(roots) = by_parent.get("") {
            for &idx in roots.iter().rev() {
                stack.push(idx);
            }
        }

        while let Some(idx) = stack.pop() {
            if !visited_idx.insert(idx) {
                continue;
            }
            let r#ref = self.nodes[idx].r#ref.clone();
            if !visited_ref.insert(r#ref.clone()) {
                // Duplicate ref — keep the first occurrence (already in
                // new_order) and skip this one.
                continue;
            }
            new_order.push(idx);
            if let Some(children) = by_parent.get(&r#ref) {
                for &cidx in children.iter().rev() {
                    stack.push(cidx);
                }
            }
        }

        if new_order.len() != self.nodes.len() {
            for idx in 0..self.nodes.len() {
                if !visited_idx.contains(&idx) {
                    let r = self.nodes[idx].r#ref.clone();
                    if visited_ref.insert(r) {
                        new_order.push(idx);
                    }
                }
            }
        }

        let mut taken: Vec<Option<CompressedNode>> = self.nodes.drain(..).map(Some).collect();
        let mut sorted: Vec<CompressedNode> = Vec::with_capacity(new_order.len());
        for idx in new_order {
            if let Some(node) = taken[idx].take() {
                sorted.push(node);
            }
        }
        self.nodes = sorted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(r#ref: &str, parent: &str, role: &str, position: usize) -> CompressedNode {
        CompressedNode {
            r#ref: r#ref.into(),
            role: role.into(),
            name: None,
            value: None,
            parent: parent.into(),
            position,
            flags: NodeFlags::default(),
        }
    }

    #[test]
    fn flags_round_trip_through_names() {
        let f = NodeFlags {
            focused: true,
            checked: true,
            ..NodeFlags::default()
        };
        let names = f.names();
        let back = NodeFlags::from_names(&names);
        assert_eq!(f, back);
    }

    #[test]
    fn flags_serialize_to_string_array() {
        let f = NodeFlags {
            focused: true,
            checked: true,
            ..NodeFlags::default()
        };
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(json, r#"["focused","checked"]"#);
    }

    #[test]
    fn empty_snapshot_is_empty() {
        let s = CompressedSnapshot::empty("https://x.test");
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn canonicalize_orders_nodes_pre_order() {
        let mut s = CompressedSnapshot {
            url: "https://x.test".into(),
            captured_at_ms: 0,
            root: String::new(),
            nodes: vec![
                n("@e3", "@e1", "button", 0),
                n("@e1", "", "WebArea", 0),
                n("@e2", "@e1", "button", 1),
            ],
        };
        s.canonicalize();
        let order: Vec<&str> = s.nodes.iter().map(|n| n.r#ref.as_str()).collect();
        assert_eq!(order, vec!["@e1", "@e3", "@e2"]);
    }

    #[test]
    fn canonicalize_is_idempotent() {
        let mut s = CompressedSnapshot {
            url: "https://x.test".into(),
            captured_at_ms: 0,
            root: String::new(),
            nodes: vec![
                n("@e1", "", "WebArea", 0),
                n("@e2", "@e1", "button", 0),
                n("@e3", "@e1", "button", 1),
            ],
        };
        s.canonicalize();
        let first = s.clone();
        s.canonicalize();
        assert_eq!(first, s);
    }
}
