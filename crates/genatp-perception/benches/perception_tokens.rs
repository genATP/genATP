//! Phase 2 benchmark: compression latency + token count over the fixture
//! corpus.
//!
//! Performance budgets (see `docs/DEVELOPMENT_PLAN.md` §Phase 2):
//!
//! - Snapshot latency p95 < 250 ms (compression alone is a fraction of that;
//!   the rest is browser CDP roundtrip).
//! - Average compressed token count < 800 across the corpus.
//!
//! Criterion reports per-fixture timings; the regression gate runs in CI via
//! `cargo bench --bench perception_tokens -- --save-baseline current` and
//! compares against the previous baseline.

use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use genatp_perception::{
    CompressOptions, ContentHash, ModelKind, RawTree, RefMap, SnapshotStore, TokenEstimator,
    compress,
};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("fixtures").join("perception").join("raw"))
        .expect("workspace root resolvable")
}

fn load_corpus() -> Vec<(String, RawTree)> {
    let dir = fixtures_dir();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("read fixtures dir")
        .filter_map(|res| res.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    entries.sort();

    entries
        .into_iter()
        .map(|path| {
            let bytes = std::fs::read(&path).expect("read fixture");
            let tree: RawTree = serde_json::from_slice(&bytes).expect("parse fixture");
            let slug = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            (slug, tree)
        })
        .collect()
}

fn bench_compress(c: &mut Criterion) {
    let corpus = load_corpus();
    let mut group = c.benchmark_group("compress");
    for (slug, tree) in &corpus {
        group.bench_function(slug.as_str(), |b| {
            b.iter(|| {
                let mut refs = RefMap::new();
                let _ = compress(
                    std::hint::black_box(tree),
                    &mut refs,
                    &CompressOptions::default(),
                );
            });
        });
    }
    group.finish();
}

fn bench_token_estimate(c: &mut Criterion) {
    let corpus = load_corpus();
    let estimator = TokenEstimator::new(ModelKind::Claude);
    let snapshots: Vec<_> = corpus
        .into_iter()
        .map(|(slug, tree)| {
            let mut refs = RefMap::new();
            (
                slug,
                compress(&tree, &mut refs, &CompressOptions::default()),
            )
        })
        .collect();

    let mut group = c.benchmark_group("token_estimate");
    for (slug, snap) in &snapshots {
        group.bench_function(slug.as_str(), |b| {
            b.iter(|| {
                let _ = estimator
                    .estimate_snapshot(std::hint::black_box(snap))
                    .expect("estimate");
            });
        });
    }
    group.finish();
}

fn bench_store_put(c: &mut Criterion) {
    let corpus = load_corpus();
    let snapshots: Vec<_> = corpus
        .into_iter()
        .map(|(slug, tree)| {
            let mut refs = RefMap::new();
            (
                slug,
                compress(&tree, &mut refs, &CompressOptions::default()),
            )
        })
        .collect();

    let mut group = c.benchmark_group("store_put");
    for (slug, snap) in &snapshots {
        group.bench_function(slug.as_str(), |b| {
            b.iter_with_setup(
                || SnapshotStore::open(":memory:").expect("open mem store"),
                |store| {
                    let _ = store.put(std::hint::black_box(snap)).expect("put");
                },
            );
        });
    }
    group.finish();
}

fn bench_content_hash(c: &mut Criterion) {
    let corpus = load_corpus();
    let snapshots: Vec<_> = corpus
        .into_iter()
        .map(|(slug, tree)| {
            let mut refs = RefMap::new();
            (
                slug,
                compress(&tree, &mut refs, &CompressOptions::default()),
            )
        })
        .collect();

    let mut group = c.benchmark_group("content_hash");
    for (slug, snap) in &snapshots {
        group.bench_function(slug.as_str(), |b| {
            b.iter(|| {
                let _ = ContentHash::of(std::hint::black_box(snap)).expect("hash");
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_compress,
    bench_token_estimate,
    bench_store_put,
    bench_content_hash
);
criterion_main!(benches);
