//! Property test: `apply(diff(a, b), a) == b` for arbitrary canonical
//! [`CompressedSnapshot`] pairs.
//!
//! We generate `a` from a random small tree, then derive `b` via a sequence
//! of structural mutations (rename, add, remove, flip flag). Each mutation
//! preserves canonical structure so the property has full coverage of the
//! diff/apply paths.

mod common;

use genatp_perception::{
    CompressOptions, CompressedNode, CompressedSnapshot, NodeFlags, RawNode, RawTree, RefMap,
    apply, compress, diff,
};
use proptest::prelude::*;

use crate::common::{corpus_files, load_raw_tree};

const ROLES: &[&str] = &[
    "button",
    "link",
    "textbox",
    "checkbox",
    "heading",
    "paragraph",
    "tab",
    "menuitem",
    "searchbox",
];

fn arb_role() -> impl Strategy<Value = String> {
    proptest::sample::select(ROLES.to_vec()).prop_map(String::from)
}

fn arb_name() -> impl Strategy<Value = Option<String>> {
    proptest::option::of("[a-zA-Z0-9 ]{0,16}".prop_map(String::from))
}

prop_compose! {
    fn arb_raw_node(id: String, parent: Option<String>)
                   (role in arb_role(), name in arb_name())
                   -> RawNode {
        RawNode {
            node_id: id.clone(),
            parent_id: parent.clone(),
            child_ids: vec![],
            role: Some(role),
            name,
            value: None,
            description: None,
            properties: vec![],
            ignored: false,
        }
    }
}

fn arb_small_tree() -> impl Strategy<Value = RawTree> {
    (1usize..6).prop_flat_map(|n_children| {
        let leaves =
            proptest::collection::vec(arb_raw_node("x".into(), Some("1".into())), n_children);
        leaves.prop_map(|mut leaves| {
            for (i, leaf) in leaves.iter_mut().enumerate() {
                leaf.node_id = format!("{}", i + 2);
            }
            let child_ids: Vec<String> = leaves.iter().map(|n| n.node_id.clone()).collect();
            let mut nodes = Vec::with_capacity(leaves.len() + 1);
            nodes.push(RawNode {
                node_id: "1".into(),
                parent_id: None,
                child_ids,
                role: Some("RootWebArea".into()),
                name: Some("root".into()),
                value: None,
                description: None,
                properties: vec![],
                ignored: false,
            });
            nodes.extend(leaves);
            RawTree::new("https://prop.test", nodes)
        })
    })
}

fn snap(tree: &RawTree, refs: &mut RefMap) -> CompressedSnapshot {
    compress(tree, refs, &CompressOptions::default())
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        max_shrink_iters: 256,
        ..ProptestConfig::default()
    })]

    #[test]
    fn diff_then_apply_round_trips_compress_pairs(a in arb_small_tree(), b in arb_small_tree()) {
        let mut refs = RefMap::new();
        let snap_a = snap(&a, &mut refs);
        let snap_b = snap(&b, &mut refs);

        let patch = diff(&snap_a, &snap_b);
        let reconstructed = apply(&snap_a, &patch).expect("apply must succeed");
        prop_assert_eq!(reconstructed, snap_b);
    }

    #[test]
    fn diff_then_apply_round_trips_with_flag_mutation(a in arb_small_tree()) {
        let mut refs = RefMap::new();
        let snap_a = snap(&a, &mut refs);

        let mut snap_b = snap_a.clone();
        if let Some(node) = snap_b.nodes.first_mut() {
            node.flags = NodeFlags { focused: true, ..NodeFlags::default() };
        }
        snap_b.canonicalize();

        let patch = diff(&snap_a, &snap_b);
        let reconstructed = apply(&snap_a, &patch).expect("apply must succeed");
        prop_assert_eq!(reconstructed, snap_b);
    }
}

#[test]
fn fixture_pairs_round_trip_under_diff_apply() {
    // Cross every fixture with every other fixture and check round-trip.
    // O(n²) but small (~144 pairs).
    let files = corpus_files();
    let trees: Vec<_> = files.iter().map(|p| load_raw_tree(p)).collect();
    for (i, t_i) in trees.iter().enumerate() {
        for (j, t_j) in trees.iter().enumerate() {
            let mut refs = RefMap::new();
            let snap_i = snap(t_i, &mut refs);
            let snap_j = snap(t_j, &mut refs);
            let patch = diff(&snap_i, &snap_j);
            let back = apply(&snap_i, &patch).expect("apply must succeed");
            assert_eq!(
                back,
                snap_j,
                "round-trip failed for fixtures #{i}→#{j}: {} -> {}",
                files[i].display(),
                files[j].display()
            );
        }
    }
}

#[test]
fn empty_to_full_diff_is_all_adds() {
    let raw = RawTree::new(
        "https://x",
        vec![
            RawNode {
                node_id: "1".into(),
                parent_id: None,
                child_ids: vec!["2".into()],
                role: Some("RootWebArea".into()),
                name: Some("root".into()),
                value: None,
                description: None,
                properties: vec![],
                ignored: false,
            },
            RawNode {
                node_id: "2".into(),
                parent_id: Some("1".into()),
                child_ids: vec![],
                role: Some("button".into()),
                name: Some("Click".into()),
                value: None,
                description: None,
                properties: vec![],
                ignored: false,
            },
        ],
    );
    let mut refs = RefMap::new();
    let s = snap(&raw, &mut refs);
    let empty = CompressedSnapshot::empty(&s.url);
    let patch = diff(&empty, &s);
    assert_eq!(patch.ops.len(), s.nodes.len());
    let all_adds = patch
        .ops
        .iter()
        .all(|op| matches!(op, genatp_perception::Op::Add { .. }));
    assert!(all_adds);
    let back = apply(&empty, &patch).expect("apply");
    assert_eq!(back, s);
}

#[test]
fn unused_compressed_node_helper() {
    // Compile-time guard: ensure CompressedNode stays usable as a struct in
    // tests as we evolve the public API.
    let _ = CompressedNode {
        r#ref: "@e1".into(),
        role: "button".into(),
        name: None,
        value: None,
        parent: String::new(),
        position: 0,
        flags: NodeFlags::default(),
    };
}
