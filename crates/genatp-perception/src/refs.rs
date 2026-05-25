//! Element ref compressor — assign stable `@e1`, `@e2`, … refs to a11y nodes.
//!
//! ### Stability contract
//!
//! Within a single page session (i.e. successive snapshots taken between
//! navigations on the same `RefMap`), a node keeps its ref iff its
//! **identity** is unchanged:
//!
//! ```text
//! identity(node) = (path_of_role_name(parent_chain), role, name, dup_index)
//! ```
//!
//! where `path_of_role_name` is the pre-order list of `(role, name)` tuples
//! from the document root down to the node's parent, and `dup_index` is the
//! 0-based count of preceding siblings under the same parent that share the
//! same `(role, name)`. This means:
//!
//! - Adding a sibling with a *different* `(role, name)` does **not** rotate
//!   any existing refs (siblings keep their positions in the dup-index
//!   ordering; the new sibling gets a fresh identity).
//! - Renaming a button (e.g. "OK" → "Confirm") **does** rotate that ref
//!   (intentional — semantic identity changed).
//! - Two sibling table cells with empty names get **distinct** refs because
//!   `dup_index` differs.
//!
//! Refs are **stable across re-runs** of the same page only if a [`RefMap`]
//! built on the previous snapshot is threaded in. Without a prior map, refs
//! are assigned in pre-order DFS starting from `@e1`.

use std::collections::{HashMap, HashSet};

use crate::raw::{RawNode, RawTree};
use crate::snapshot::Ref;

/// One segment of the ancestor path used in identity keys: `(role, name, dup_index)`.
type PathSegment = (String, String, usize);

/// Carries identity → ref bindings across snapshots in the same session.
#[derive(Debug, Clone, Default)]
pub struct RefMap {
    /// Identity key → assigned ref.
    bindings: HashMap<String, Ref>,
    /// Highest `@eN` index ever assigned in this map. New refs use
    /// `next_index + 1` so nothing collides with retired refs.
    next_index: u32,
}

impl RefMap {
    /// Create a fresh map. Refs start at `@e1`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of bindings currently held (live + retired).
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// `true` if no bindings.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Greatest ref index assigned so far. `0` if no refs have been minted.
    #[must_use]
    pub fn high_water(&self) -> u32 {
        self.next_index
    }

    /// Look up the ref for a given identity, if any.
    #[must_use]
    pub fn get(&self, identity: &str) -> Option<&Ref> {
        self.bindings.get(identity)
    }

    /// Assign a ref for `identity`, reusing the existing ref if present.
    fn assign(&mut self, identity: String) -> Ref {
        if let Some(existing) = self.bindings.get(&identity) {
            return existing.clone();
        }
        self.next_index += 1;
        let r#ref = format!("@e{}", self.next_index);
        self.bindings.insert(identity, r#ref.clone());
        r#ref
    }

    /// Run the assignment pass over a stripped raw tree, producing a parallel
    /// `node_id → Ref` map that downstream compression uses to populate
    /// `CompressedNode::ref` / `parent`.
    ///
    /// `tree` must already be stripped (see [`crate::strip`]). Nodes with no
    /// role are skipped silently.
    pub fn assign_for_tree(&mut self, tree: &RawTree) -> HashMap<String, Ref> {
        let mut node_to_ref: HashMap<String, Ref> = HashMap::new();
        let by_id: HashMap<&str, &RawNode> =
            tree.nodes.iter().map(|n| (n.node_id.as_str(), n)).collect();

        // Pass 1: compute dup_index per node — how many preceding siblings
        // share the same (role, name) key. Stable across additions because
        // we use document-order child indices (not running counters).
        let mut dup_index: HashMap<String, usize> = HashMap::new();
        for parent in &tree.nodes {
            let mut counts: HashMap<(Option<&str>, &str), usize> = HashMap::new();
            for cid in &parent.child_ids {
                let Some(child) = by_id.get(cid.as_str()) else {
                    continue;
                };
                let role = child.role.as_deref();
                let name = child.name.as_deref().unwrap_or("");
                let entry = counts.entry((role, name)).or_insert(0);
                dup_index.insert(cid.clone(), *entry);
                *entry += 1;
            }
        }
        let Some(root_id) = tree.root_id() else {
            return node_to_ref;
        };
        dup_index.entry(root_id.to_string()).or_insert(0);

        // Pass 2: pre-order DFS from the root. We assign in document order so
        // ties on `(role, name)` are broken via the precomputed dup_index.
        let mut visited: HashSet<String> = HashSet::new();
        let mut stack: Vec<(String, Vec<PathSegment>)> = vec![(root_id.to_string(), Vec::new())];

        while let Some((id, path)) = stack.pop() {
            if !visited.insert(id.clone()) {
                continue;
            }
            let Some(node) = by_id.get(id.as_str()) else {
                continue;
            };
            let Some(role) = node.role.as_deref() else {
                continue;
            };
            let name = node.name.clone().unwrap_or_default();
            let dup = *dup_index.get(&id).unwrap_or(&0);
            let identity = identity_key(&path, role, &name, dup);
            let r#ref = self.assign(identity);
            node_to_ref.insert(id.clone(), r#ref);

            let mut child_path = path.clone();
            child_path.push((role.to_string(), name, dup));

            for child_id in node.child_ids.iter().rev() {
                stack.push((child_id.clone(), child_path.clone()));
            }
        }
        node_to_ref
    }
}

/// Build the deterministic identity string for one node.
///
/// We use a unit-separator (`\u{1F}`) between fields and a record-separator
/// (`\u{1E}`) between path segments, so role/name/dup strings stay
/// unambiguous even when names contain punctuation.
fn identity_key(path: &[PathSegment], role: &str, name: &str, dup: usize) -> String {
    let mut s = String::with_capacity(64 + path.len() * 16);
    for (r, n, d) in path {
        s.push_str(r);
        s.push('\u{1F}');
        s.push_str(n);
        s.push('\u{1F}');
        s.push_str(&d.to_string());
        s.push('\u{1E}');
    }
    s.push_str(role);
    s.push('\u{1F}');
    s.push_str(name);
    s.push('\u{1F}');
    s.push_str(&dup.to_string());
    s
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
    fn fresh_map_assigns_e1_to_root() {
        let tree = RawTree::new("x", vec![node("1", None, &["2"], "WebArea", "Page")]);
        let mut map = RefMap::new();
        let assigned = map.assign_for_tree(&tree);
        assert_eq!(assigned["1"], "@e1");
    }

    #[test]
    fn refs_are_stable_across_recompress() {
        let tree = RawTree::new(
            "x",
            vec![
                node("1", None, &["2", "3"], "WebArea", ""),
                node("2", Some("1"), &[], "button", "OK"),
                node("3", Some("1"), &[], "button", "Cancel"),
            ],
        );
        let mut map = RefMap::new();
        let first = map.assign_for_tree(&tree);
        let second = map.assign_for_tree(&tree);
        assert_eq!(first, second);
        assert_eq!(map.high_water(), 3);
    }

    #[test]
    fn renaming_a_button_rotates_only_that_ref() {
        let tree_a = RawTree::new(
            "x",
            vec![
                node("1", None, &["2", "3"], "WebArea", ""),
                node("2", Some("1"), &[], "button", "OK"),
                node("3", Some("1"), &[], "button", "Cancel"),
            ],
        );
        let tree_b = RawTree::new(
            "x",
            vec![
                node("1", None, &["2", "3"], "WebArea", ""),
                node("2", Some("1"), &[], "button", "Confirm"), // renamed
                node("3", Some("1"), &[], "button", "Cancel"),
            ],
        );
        let mut map = RefMap::new();
        let a = map.assign_for_tree(&tree_a);
        let b = map.assign_for_tree(&tree_b);
        assert_eq!(a["1"], b["1"]);
        assert_eq!(a["3"], b["3"]);
        assert_ne!(a["2"], b["2"]); // OK ≠ Confirm: ref rotates
    }

    #[test]
    fn appending_a_sibling_does_not_rotate_existing_refs() {
        let tree_a = RawTree::new(
            "x",
            vec![
                node("1", None, &["2"], "WebArea", ""),
                node("2", Some("1"), &[], "button", "OK"),
            ],
        );
        let tree_b = RawTree::new(
            "x",
            vec![
                node("1", None, &["2", "3"], "WebArea", ""),
                node("2", Some("1"), &[], "button", "OK"),
                node("3", Some("1"), &[], "button", "Cancel"),
            ],
        );
        let mut map = RefMap::new();
        let a = map.assign_for_tree(&tree_a);
        let b = map.assign_for_tree(&tree_b);
        assert_eq!(a["1"], b["1"]);
        assert_eq!(a["2"], b["2"]);
        assert!(b.contains_key("3"));
    }
}
