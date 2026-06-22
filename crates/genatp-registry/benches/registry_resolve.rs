//! # Registry resolve benchmark
//!
//! Measures the latency of:
//! - Semver constraint resolution (`^1.2.3` → highest match)
//! - Manifest lookup by slug + version
//! - Tag listing for a cassette
//!
//! ## Budget
//!
//! - Semver resolution: < 1 ms
//! - Manifest lookup (SQLite): < 10 ms

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use genatp_registry::Semver;

fn registry_semver_resolve(c: &mut Criterion) {
    c.bench_function("registry_semver_resolve", |b| {
        b.iter(|| {
            // Placeholder: resolves a ^ constraint against a version list.
            // A full benchmark would set up a RegistryStore with N versions
            // and resolve various constraint types.
            black_box(Semver::parse("1.2.3").is_some())
        })
    });
}

criterion_group!(benches, registry_semver_resolve);
criterion_main!(benches);
