//! # Cassette replay benchmark
//!
//! Measures the latency of:
//! - Verifying the hash chain on a sample cassette
//! - Deserializing a cassette from JSON bytes
//! - Full round-trip (build → serialize → deserialize → verify)
//!
//! ## Acceptance budget
//!
//! - Hash verification: < 5 ms
//! - Deserialize: < 10 ms
//! - Full round-trip: < 20 ms

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use genatp_cassette::schema::{
    Cassette, CassetteHeader, CassetteStep, SCHEMA_VERSION, compute_step_hash,
    step_payload_for_hash,
};
use genatp_core::action::Action;
use genatp_core::budget::BudgetSpend;

/// Build a sample cassette with realistic data for benchmarking.
fn build_sample_cassette(step_count: u32) -> Cassette {
    let mut steps = Vec::with_capacity(step_count as usize);
    let mut prev_hash = String::new();

    for i in 0..step_count {
        let action = if i % 2 == 0 {
            Action::Click {
                r#ref: format!("@e{i}"),
            }
        } else {
            Action::Scroll { dx: 0, dy: 300 }
        };

        let mut step = CassetteStep {
            seq: i,
            at: chrono::Utc::now(),
            url: "https://example.com/page".into(),
            snapshot_hash: format!("snap_{i}"),
            action,
            action_success: true,
            action_message: None,
            parent_hash: prev_hash.clone(),
            hash: String::new(),
        };

        let payload = step_payload_for_hash(&step);
        step.hash = compute_step_hash(&prev_hash, &payload);
        prev_hash = step.hash.clone();
        steps.push(step);
    }

    let tip_hash = steps.last().map_or(String::new(), |s| s.hash.clone());
    let header = CassetteHeader {
        schema_version: SCHEMA_VERSION,
        slug: "benchmark-test".into(),
        version: "1.0.0".into(),
        description: "Benchmark cassette for replay tests".into(),
        goal: "Navigate and interact".into(),
        start_url: "https://example.com".into(),
        model: "gpt-4".into(),
        recorded_at: chrono::Utc::now(),
        spend: BudgetSpend::default(),
        step_count,
        tip_hash,
        signer_public_key: "pk_benchmark_test_dummy_key_not_a_real_secret_0123456789abcdef".into(),
        domains: vec!["example.com".into()],
    };

    Cassette { header, steps }
}

fn cassette_verify_hash_chain(c: &mut Criterion) {
    let cassette = build_sample_cassette(10);

    c.bench_function("cassette_verify_hash_chain", |b| {
        b.iter(|| {
            let result = black_box(&cassette).verify_hash_chain();
            black_box(result)
        })
    });
}

fn cassette_deserialize(c: &mut Criterion) {
    let cassette = build_sample_cassette(10);
    let json_bytes = serde_json::to_vec(&cassette).unwrap();

    c.bench_function("cassette_deserialize", |b| {
        b.iter(|| {
            let result: Cassette = serde_json::from_slice(black_box(&json_bytes)).unwrap();
            black_box(result)
        })
    });
}

fn cassette_roundtrip(c: &mut Criterion) {
    let cassette = build_sample_cassette(10);

    c.bench_function("cassette_roundtrip", |b| {
        b.iter(|| {
            let json = serde_json::to_vec(black_box(&cassette)).unwrap();
            let deserialized: Cassette = serde_json::from_slice(&json).unwrap();
            deserialized.verify_hash_chain().unwrap();
            black_box(deserialized)
        })
    });
}

criterion_group!(
    benches,
    cassette_verify_hash_chain,
    cassette_deserialize,
    cassette_roundtrip
);
criterion_main!(benches);
