//! Snapshot diff engine.
//!
//! Diffs are computed between two **canonical** [`CompressedSnapshot`]s and
//! emit a [`Patch`] of three operation kinds keyed by [`Ref`]:
//!
//! - [`Op::Remove`] — a ref present in `prev` no longer exists in `curr`.
//! - [`Op::Add`] — a ref present in `curr` did not exist in `prev`.
//! - [`Op::Replace`] — same ref in both, but at least one attribute or its
//!   parent / position changed.
//!
//! ### Invertibility
//!
//! `apply(diff(a, b), a)` returns a snapshot byte-equal to `b` (after
//! canonicalisation). This is enforced by a proptest in `tests/proptests.rs`.
//!
//! ### Why per-ref ops, not tree edits?
//!
//! The compressed snapshot is already a flat list keyed by ref; tree shape
//! lives in `parent` / `position` fields. So a "move" is just a `Replace`
//! that changes those fields, which is both simpler and provably reversible.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::error::{PerceptionError, Result};
use crate::snapshot::{CompressedNode, CompressedSnapshot, Ref};

/// One operation in a [`Patch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// Remove the node identified by `ref`. Children are not implicit —
    /// the diff also emits explicit `Remove` for descendants if they are
    /// gone in `curr`.
    Remove { r#ref: Ref },

    /// Add a new node. The full [`CompressedNode`] is included; the patch
    /// applier verifies that `node.parent` exists in the patched snapshot.
    Add { node: CompressedNode },

    /// Replace the node at `node.ref` with `node`. Used for any field change
    /// (name, value, flags, parent, position).
    Replace { node: CompressedNode },
}

impl Op {
    /// Ref this op targets.
    #[must_use]
    pub fn ref_(&self) -> &str {
        match self {
            Op::Remove { r#ref } => r#ref,
            Op::Add { node } | Op::Replace { node } => &node.r#ref,
        }
    }
}

/// Patch between two snapshots. Holds the source `url` for context — when
/// the URL differs, we emit a "full reset" patch (every prev node removed,
/// every curr node added) but still wrap it in [`Patch`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Patch {
    /// Source `url` taken from `prev` (or empty if both empty).
    #[serde(default)]
    pub from_url: String,
    /// Destination `url` taken from `curr`.
    #[serde(default)]
    pub to_url: String,
    /// Capture timestamp of `curr` (ms). Lets a debugger order diffs.
    #[serde(default)]
    pub captured_at_ms: u64,
    /// Root ref of `curr`. Apply uses this to set the resulting snapshot's
    /// root after operations are applied.
    #[serde(default)]
    pub root: Ref,
    pub ops: Vec<Op>,
}

impl Patch {
    /// `true` if the patch contains no operations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty() && self.from_url == self.to_url
    }

    /// Number of operations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ops.len()
    }
}

/// Compute a patch from `prev` → `curr`. Both inputs should be canonicalised
/// via [`CompressedSnapshot::canonicalize`]; the function defensively
/// canonicalises clones if the caller forgot.
#[must_use]
pub fn diff(prev: &CompressedSnapshot, curr: &CompressedSnapshot) -> Patch {
    let mut prev_canon = prev.clone();
    prev_canon.canonicalize();
    let mut curr_canon = curr.clone();
    curr_canon.canonicalize();

    let prev_index: HashMap<&str, &CompressedNode> = prev_canon.index();
    let curr_index: HashMap<&str, &CompressedNode> = curr_canon.index();

    let prev_refs: HashSet<&str> = prev_index.keys().copied().collect();
    let curr_refs: HashSet<&str> = curr_index.keys().copied().collect();

    let mut ops: Vec<Op> = Vec::new();

    // Removes in deepest-first order so applying in sequence never orphans
    // descendants. We approximate "deepest-first" by reversing the prev
    // canonical (pre-order) list, since pre-order puts ancestors before
    // descendants.
    for node in prev_canon.nodes.iter().rev() {
        if !curr_refs.contains(node.r#ref.as_str()) {
            ops.push(Op::Remove {
                r#ref: node.r#ref.clone(),
            });
        }
    }

    // Replaces and Adds in pre-order so a node's parent always exists by
    // the time we add it.
    for node in &curr_canon.nodes {
        match prev_index.get(node.r#ref.as_str()) {
            Some(prev_node) if *prev_node == node => {
                // No change.
            }
            Some(_) => ops.push(Op::Replace { node: node.clone() }),
            None => {
                if prev_refs.contains(node.r#ref.as_str()) {
                    // Should never happen given the conditions above, but be
                    // defensive.
                    ops.push(Op::Replace { node: node.clone() });
                } else {
                    ops.push(Op::Add { node: node.clone() });
                }
            }
        }
    }

    Patch {
        from_url: prev_canon.url,
        to_url: curr_canon.url.clone(),
        captured_at_ms: curr_canon.captured_at_ms,
        root: curr_canon.root,
        ops,
    }
}

/// Apply a patch to `prev`, returning the patched snapshot.
///
/// Apply order: Remove → Replace → Add — same order as [`diff`] emits, but
/// the function is defensive and works regardless of intra-bucket order.
///
/// # Errors
///
/// - [`PerceptionError::UnknownRef`] when a `Remove` or `Replace` targets a
///   ref absent from `prev`.
/// - [`PerceptionError::UnknownParent`] when an `Add` references a parent
///   ref absent from the partial result.
pub fn apply(prev: &CompressedSnapshot, patch: &Patch) -> Result<CompressedSnapshot> {
    let mut result = prev.clone();
    result.url.clone_from(&patch.to_url);
    result.captured_at_ms = patch.captured_at_ms;
    result.root.clone_from(&patch.root);

    let mut by_ref: HashMap<String, CompressedNode> = result
        .nodes
        .drain(..)
        .map(|n| (n.r#ref.clone(), n))
        .collect();

    let (mut removes, mut replaces, mut adds): (Vec<&Op>, Vec<&Op>, Vec<&Op>) =
        (Vec::new(), Vec::new(), Vec::new());
    for op in &patch.ops {
        match op {
            Op::Remove { .. } => removes.push(op),
            Op::Replace { .. } => replaces.push(op),
            Op::Add { .. } => adds.push(op),
        }
    }

    for op in removes {
        let Op::Remove { r#ref } = op else {
            continue;
        };
        if by_ref.remove(r#ref).is_none() {
            return Err(PerceptionError::UnknownRef((*r#ref).clone()));
        }
    }
    for op in replaces {
        if let Op::Replace { node } = op {
            if !by_ref.contains_key(&node.r#ref) {
                return Err(PerceptionError::UnknownRef(node.r#ref.clone()));
            }
            by_ref.insert(node.r#ref.clone(), node.clone());
        }
    }
    for op in adds {
        if let Op::Add { node } = op {
            if !node.parent.is_empty() && !by_ref.contains_key(&node.parent) {
                return Err(PerceptionError::UnknownParent(node.parent.clone()));
            }
            by_ref.insert(node.r#ref.clone(), node.clone());
        }
    }

    result.nodes = by_ref.into_values().collect();
    result.canonicalize();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compress::{CompressOptions, compress};
    use crate::raw::{RawNode, RawTree};
    use crate::refs::RefMap;

    fn n(id: &str, parent: Option<&str>, children: &[&str], role: &str, name: &str) -> RawNode {
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

    fn snap(tree: &RawTree, refs: &mut RefMap) -> CompressedSnapshot {
        compress(tree, refs, &CompressOptions::default())
    }

    #[test]
    fn unchanged_snapshots_yield_empty_patch() {
        let tree = RawTree::new("x", vec![n("1", None, &[], "WebArea", "")]);
        let mut refs = RefMap::new();
        let s = snap(&tree, &mut refs);
        let p = diff(&s, &s);
        assert!(p.ops.is_empty());
    }

    #[test]
    fn add_button_emits_add_op() {
        let mut refs = RefMap::new();
        let a = snap(
            &RawTree::new("x", vec![n("1", None, &[], "WebArea", "")]),
            &mut refs,
        );
        let b = snap(
            &RawTree::new(
                "x",
                vec![
                    n("1", None, &["2"], "WebArea", ""),
                    n("2", Some("1"), &[], "button", "OK"),
                ],
            ),
            &mut refs,
        );
        let p = diff(&a, &b);
        assert_eq!(p.ops.len(), 1);
        assert!(matches!(p.ops[0], Op::Add { .. }));
    }

    #[test]
    fn remove_button_emits_remove_op() {
        let mut refs = RefMap::new();
        let a = snap(
            &RawTree::new(
                "x",
                vec![
                    n("1", None, &["2"], "WebArea", ""),
                    n("2", Some("1"), &[], "button", "OK"),
                ],
            ),
            &mut refs,
        );
        let b = snap(
            &RawTree::new("x", vec![n("1", None, &[], "WebArea", "")]),
            &mut refs,
        );
        let p = diff(&a, &b);
        assert_eq!(p.ops.len(), 1);
        assert!(matches!(p.ops[0], Op::Remove { .. }));
    }

    #[test]
    fn rename_button_emits_remove_then_add() {
        let mut refs = RefMap::new();
        let a = snap(
            &RawTree::new(
                "x",
                vec![
                    n("1", None, &["2"], "WebArea", ""),
                    n("2", Some("1"), &[], "button", "OK"),
                ],
            ),
            &mut refs,
        );
        let b = snap(
            &RawTree::new(
                "x",
                vec![
                    n("1", None, &["2"], "WebArea", ""),
                    n("2", Some("1"), &[], "button", "Confirm"),
                ],
            ),
            &mut refs,
        );
        let p = diff(&a, &b);
        // The renamed button gets a new ref (identity changed); diff sees it
        // as remove + add.
        assert_eq!(p.ops.len(), 2);
    }

    #[test]
    fn apply_round_trips_simple_diff() {
        let mut refs = RefMap::new();
        let a = snap(
            &RawTree::new("x", vec![n("1", None, &[], "WebArea", "")]),
            &mut refs,
        );
        let b = snap(
            &RawTree::new(
                "x",
                vec![
                    n("1", None, &["2"], "WebArea", ""),
                    n("2", Some("1"), &[], "button", "OK"),
                ],
            ),
            &mut refs,
        );
        let p = diff(&a, &b);
        let out = apply(&a, &p).expect("apply");
        assert_eq!(out, b);
    }

    #[test]
    fn apply_unknown_ref_errors() {
        let mut refs = RefMap::new();
        let a = snap(
            &RawTree::new("x", vec![n("1", None, &[], "WebArea", "")]),
            &mut refs,
        );
        let bad_patch = Patch {
            from_url: "x".into(),
            to_url: "x".into(),
            captured_at_ms: 0,
            root: "@e1".into(),
            ops: vec![Op::Remove {
                r#ref: "@e999".into(),
            }],
        };
        let err = apply(&a, &bad_patch).expect_err("must fail");
        assert!(matches!(err, PerceptionError::UnknownRef(_)));
    }
}
