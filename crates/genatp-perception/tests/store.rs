//! Integration test: open a real on-disk SQLite store, write each fixture
//! snapshot, reload, and verify round-trip + dedup behaviour.

mod common;

use genatp_perception::{CompressOptions, RefMap, SnapshotStore, compress};
use tempfile::TempDir;

use crate::common::{corpus_files, load_raw_tree};

#[test]
fn fixture_corpus_round_trips_through_disk_store() {
    let dir = TempDir::new().expect("tempdir");
    let store = SnapshotStore::open(dir.path().join("snapshots.db")).expect("open store");

    let files = corpus_files();
    assert!(
        !files.is_empty(),
        "fixture corpus must contain ≥ 1 raw a11y tree"
    );

    let mut hashes = Vec::with_capacity(files.len());
    for path in &files {
        let raw = load_raw_tree(path);
        let mut refs = RefMap::new();
        let snap = compress(&raw, &mut refs, &CompressOptions::default());
        let hash = store.put(&snap).expect("put");
        hashes.push((hash, snap));
    }

    assert_eq!(store.len().expect("len"), files.len() as u64);

    for (hash, original) in &hashes {
        let loaded = store.get(hash).expect("get").expect("must exist");
        assert_eq!(loaded, *original, "round-trip mismatch for {hash}");
    }

    // Re-putting all fixtures should be a no-op for storage (dedup).
    for (_, snap) in &hashes {
        let _ = store.put(snap).expect("re-put");
    }
    assert_eq!(
        store.len().expect("len"),
        files.len() as u64,
        "duplicate puts must not create new rows"
    );
}
