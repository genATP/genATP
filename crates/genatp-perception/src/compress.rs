//! Compression pipeline: raw a11y tree → [`CompressedSnapshot`].
//!
//! Pipeline:
//!
//! 1. **Strip** decorative / ignored / empty noise (see [`crate::strip`]).
//! 2. **Assign refs** with stability against the supplied [`RefMap`]
//!    (see [`crate::refs`]).
//! 3. **Project** each surviving raw node into a [`CompressedNode`] with
//!    parent ref, position, and trimmed flags.
//! 4. **Canonicalise** the node list (pre-order DFS).
//!
//! The pipeline is deterministic given a fixed `(RawTree, RefMap, StripPolicy)`.

use std::collections::HashMap;

use crate::raw::{RawNode, RawTree};
use crate::refs::RefMap;
use crate::snapshot::{CompressedNode, CompressedSnapshot, NodeFlags};
use crate::strip::{StripPolicy, strip};

/// Options for one compression pass. Default = production policy.
#[derive(Debug, Clone)]
pub struct CompressOptions {
    pub strip_policy: StripPolicy,
    /// If `true`, drop `name` strings longer than this many chars (keeps the
    /// first `truncate_name_to - 1` chars + `'…'`). `0` disables truncation.
    pub truncate_name_to: usize,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            strip_policy: StripPolicy::default(),
            truncate_name_to: 200,
        }
    }
}

/// Compress a raw a11y tree, threading a [`RefMap`] for cross-snapshot
/// stability.
///
/// `refs` is mutated in place: new identities get fresh `@eN` refs;
/// previously-seen identities keep their original ref.
#[must_use]
pub fn compress(tree: &RawTree, refs: &mut RefMap, opts: &CompressOptions) -> CompressedSnapshot {
    let stripped = strip(tree, &opts.strip_policy);
    let assigned = refs.assign_for_tree(&stripped);

    let by_id: HashMap<&str, &RawNode> = stripped
        .nodes
        .iter()
        .map(|n| (n.node_id.as_str(), n))
        .collect();

    let mut child_position: HashMap<String, usize> = HashMap::new();
    for n in &stripped.nodes {
        for (idx, cid) in n.child_ids.iter().enumerate() {
            child_position.insert(cid.clone(), idx);
        }
    }

    let mut nodes: Vec<CompressedNode> = Vec::with_capacity(stripped.nodes.len());
    for raw in &stripped.nodes {
        let Some(r#ref) = assigned.get(&raw.node_id) else {
            continue;
        };
        let parent_ref = raw
            .parent_id
            .as_deref()
            .and_then(|pid| assigned.get(pid))
            .cloned()
            .unwrap_or_default();
        let position = *child_position.get(&raw.node_id).unwrap_or(&0);

        let role = raw.role.clone().unwrap_or_default();
        let name = raw
            .name
            .as_ref()
            .map(|s| trim_name(s, opts.truncate_name_to));
        let value = raw
            .value
            .as_ref()
            .map(|s| trim_name(s, opts.truncate_name_to));
        let flags = flags_from_properties(raw, &by_id);

        nodes.push(CompressedNode {
            r#ref: r#ref.clone(),
            role,
            name,
            value,
            parent: parent_ref,
            position,
            flags,
        });
    }

    let root_ref = stripped
        .root_id()
        .and_then(|rid| assigned.get(rid))
        .cloned()
        .unwrap_or_default();

    let mut snapshot = CompressedSnapshot {
        url: stripped.url,
        captured_at_ms: stripped.captured_at_ms,
        root: root_ref.clone(),
        nodes,
    };
    // The compressed snapshot stores `parent = ""` for the root; canonicalise
    // expects the same convention.
    for n in &mut snapshot.nodes {
        if n.r#ref == root_ref {
            n.parent.clear();
            n.position = 0;
        }
    }
    snapshot.canonicalize();
    snapshot
}

fn trim_name(s: &str, max: usize) -> String {
    if max == 0 || s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn flags_from_properties(node: &RawNode, _by_id: &HashMap<&str, &RawNode>) -> NodeFlags {
    let mut f = NodeFlags::default();
    for p in &node.properties {
        let v_true = matches!(
            p.value.as_str(),
            "true" | "True" | "TRUE" | "1" | "yes" | "on"
        );
        match p.name.as_str() {
            "focused" if v_true => f.focused = true,
            "selected" if v_true => f.selected = true,
            "checked" if v_true || p.value == "mixed" => f.checked = true,
            "expanded" if v_true => f.expanded = true,
            "disabled" if v_true => f.disabled = true,
            "required" if v_true => f.required = true,
            "invalid" if v_true || p.value == "spelling" || p.value == "grammar" => {
                f.invalid = true;
            }
            "readonly" if v_true => f.readonly = true,
            _ => {}
        }
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::{RawNode, RawProperty};

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
    fn compress_simple_tree_yields_canonical_snapshot() {
        let tree = RawTree::new(
            "https://x.test/page",
            vec![
                node("1", None, &["2", "3"], "WebArea", "Demo"),
                node("2", Some("1"), &[], "button", "OK"),
                node("3", Some("1"), &[], "button", "Cancel"),
            ],
        );
        let mut refs = RefMap::new();
        let snap = compress(&tree, &mut refs, &CompressOptions::default());
        assert_eq!(snap.nodes.len(), 3);
        assert_eq!(snap.root, "@e1");
        let order: Vec<&str> = snap.nodes.iter().map(|n| n.r#ref.as_str()).collect();
        assert_eq!(order, vec!["@e1", "@e2", "@e3"]);
        assert_eq!(snap.find("@e2").unwrap().parent, "@e1");
    }

    #[test]
    fn focused_property_lifts_to_flag() {
        let mut btn = node("2", Some("1"), &[], "button", "OK");
        btn.properties = vec![RawProperty::new("focused", "true")];
        let tree = RawTree::new("x", vec![node("1", None, &["2"], "WebArea", ""), btn]);
        let mut refs = RefMap::new();
        let snap = compress(&tree, &mut refs, &CompressOptions::default());
        let b = snap.find("@e2").unwrap();
        assert!(b.flags.focused);
    }

    #[test]
    fn long_names_are_truncated_with_ellipsis() {
        let long = "x".repeat(500);
        let tree = RawTree::new(
            "x",
            vec![
                node("1", None, &["2"], "WebArea", ""),
                node("2", Some("1"), &[], "button", &long),
            ],
        );
        let mut refs = RefMap::new();
        let snap = compress(&tree, &mut refs, &CompressOptions::default());
        let b = snap.find("@e2").unwrap();
        let n = b.name.as_deref().unwrap();
        assert!(n.ends_with('…'));
        assert!(n.chars().count() <= 200);
    }

    #[test]
    fn decorative_wrappers_collapse_in_compressed_form() {
        let tree = RawTree::new(
            "x",
            vec![
                node("1", None, &["2"], "WebArea", ""),
                node("2", Some("1"), &["3"], "generic", ""),
                node("3", Some("2"), &[], "button", "Click"),
            ],
        );
        let mut refs = RefMap::new();
        let snap = compress(&tree, &mut refs, &CompressOptions::default());
        assert_eq!(snap.nodes.len(), 2);
        assert_eq!(snap.find("@e2").unwrap().parent, "@e1");
    }
}
