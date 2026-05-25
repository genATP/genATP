//! Shared helpers for `genatp-perception` integration tests.
//!
//! The corpus loader resolves files in `<workspace>/fixtures/perception/raw/`
//! and deserialises them into [`genatp_perception::RawTree`].

#![allow(dead_code)] // each test binary uses a subset of helpers

use std::path::PathBuf;

use genatp_perception::RawTree;

/// Workspace-relative path to the perception fixture corpus.
#[must_use]
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("fixtures").join("perception").join("raw"))
        .expect("workspace root resolvable")
}

/// All fixture files, sorted alphabetically by file stem so insta golden
/// names are stable across machines / filesystems.
pub fn corpus_files() -> Vec<PathBuf> {
    let dir = fixtures_dir();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {} failed: {e}", dir.display()))
        .filter_map(|res| res.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    entries.sort();
    entries
}

/// Load and parse one fixture file.
pub fn load_raw_tree(path: &std::path::Path) -> RawTree {
    let bytes =
        std::fs::read(path).unwrap_or_else(|e| panic!("read {} failed: {e}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse {} failed: {e}", path.display()))
}

/// Convert a fixture path into a stable, alphanumeric snapshot name.
pub fn slug_for(path: &std::path::Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}
