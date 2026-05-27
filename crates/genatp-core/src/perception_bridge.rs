//! Bridge from [`genatp_browser::AccessibilityTree`] to
//! [`genatp_perception::RawTree`].
//!
//! Phase 2's perception engine is deliberately decoupled from `genatp-browser`
//! — it consumes a `RawTree`, not CDP types. This module is the only place
//! in the workspace allowed to translate between them.

use genatp_browser::{AccessibilityTree, AxNodeJson};
use genatp_perception::raw::{RawNode, RawProperty, RawTree};

/// Convert a CDP-derived [`AccessibilityTree`] into a perception [`RawTree`].
///
/// The bridge:
/// - copies parent/child relationships verbatim;
/// - lower-cases property names (CDP returns mixed case);
/// - preserves the `ignored` flag so the strip stage can act on it;
/// - leaves `captured_at_ms = 0` — callers can set this with the actual
///   capture time if they care.
#[must_use]
pub fn raw_tree_from_accessibility(url: impl Into<String>, tree: &AccessibilityTree) -> RawTree {
    let nodes = tree.nodes().iter().map(map_node).collect();
    RawTree {
        url: url.into(),
        captured_at_ms: chrono::Utc::now().timestamp_millis().max(0) as u64,
        nodes,
    }
}

fn map_node(node: &AxNodeJson) -> RawNode {
    RawNode {
        node_id: node.node_id.clone(),
        parent_id: node.parent_id.clone(),
        child_ids: node.child_ids.clone(),
        role: node.role.clone(),
        name: node.name.clone(),
        value: node.value.clone(),
        description: node.description.clone(),
        properties: node
            .properties
            .iter()
            .map(|p| RawProperty::new(p.name.clone(), p.value.clone()))
            .collect(),
        ignored: node.ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genatp_browser::cdp::AxPropertyJson;

    fn axnode(
        id: &str,
        parent: Option<&str>,
        children: &[&str],
        role: Option<&str>,
        name: Option<&str>,
    ) -> AxNodeJson {
        AxNodeJson {
            node_id: id.into(),
            parent_id: parent.map(str::to_owned),
            child_ids: children.iter().map(|s| (*s).to_string()).collect(),
            role: role.map(str::to_owned),
            name: name.map(str::to_owned),
            value: None,
            description: None,
            properties: vec![AxPropertyJson {
                name: "focused".into(),
                value: "true".into(),
            }],
            backend_dom_node_id: None,
            ignored: false,
        }
    }

    #[test]
    fn maps_tree_preserving_structure() {
        let tree = AccessibilityTree::from_nodes(vec![
            axnode("1", None, &["2"], Some("WebArea"), Some("Page")),
            axnode("2", Some("1"), &[], Some("button"), Some("OK")),
        ]);
        let raw = raw_tree_from_accessibility("https://example.com", &tree);
        assert_eq!(raw.url, "https://example.com");
        assert_eq!(raw.nodes.len(), 2);
        assert_eq!(raw.nodes[0].child_ids, vec!["2".to_string()]);
        assert_eq!(raw.nodes[1].parent_id.as_deref(), Some("1"));
        assert_eq!(raw.nodes[0].properties.len(), 1);
        assert_eq!(raw.nodes[0].properties[0].name, "focused");
    }
}
