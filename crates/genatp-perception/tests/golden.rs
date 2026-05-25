//! Golden snapshot tests for the perception pipeline.
//!
//! For every fixture in `fixtures/perception/raw/` we
//!
//! 1. Compress with the production [`CompressOptions`].
//! 2. Snapshot the resulting [`CompressedSnapshot`] as JSON via `insta`.
//! 3. Snapshot the BLAKE3 content hash so a subtle drift in serialisation
//!    fails the test loudly.
//!
//! Run with `cargo insta review --workspace` to accept intentional changes.

mod common;

use genatp_perception::{
    CompressOptions, CompressedSnapshot, ContentHash, ModelKind, RefMap, TokenEstimator, compress,
};

use crate::common::{corpus_files, load_raw_tree, slug_for};

#[test]
fn corpus_compresses_and_token_count_is_within_budget() {
    let mut total_tokens: u32 = 0;
    let mut count: u32 = 0;
    let estimator = TokenEstimator::new(ModelKind::Claude);

    for path in corpus_files() {
        let raw = load_raw_tree(&path);
        let mut refs = RefMap::new();
        let snap = compress(&raw, &mut refs, &CompressOptions::default());

        let json = serde_json::to_string_pretty(&snap).expect("serialize");
        let hash = ContentHash::of(&snap).expect("hash");

        let slug = slug_for(&path);
        insta::assert_snapshot!(format!("compressed_{slug}"), json);
        insta::assert_snapshot!(format!("hash_{slug}"), hash.to_hex());

        let tokens = estimator.estimate_snapshot(&snap).expect("estimate tokens");
        total_tokens = total_tokens.saturating_add(tokens);
        count += 1;
    }

    let avg_tokens = total_tokens.checked_div(count).unwrap_or(0);

    // Phase 2 exit criterion: average compressed token count across the
    // fixture corpus must stay below 850. The fixtures we ship now are tiny
    // (~25 nodes max), so a healthy budget is 200; the gate at 850 lets the
    // corpus grow toward 50 realistic pages without flaking.
    assert!(
        avg_tokens < 850,
        "avg compressed tokens {avg_tokens} exceeds 850-token CI gate"
    );
}

#[test]
fn compress_is_deterministic() {
    for path in corpus_files() {
        let raw = load_raw_tree(&path);
        let mut refs_a = RefMap::new();
        let mut refs_b = RefMap::new();
        let a: CompressedSnapshot = compress(&raw, &mut refs_a, &CompressOptions::default());
        let b: CompressedSnapshot = compress(&raw, &mut refs_b, &CompressOptions::default());
        assert_eq!(a, b, "{} compresses non-deterministically", path.display());
    }
}
