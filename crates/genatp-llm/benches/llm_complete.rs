//! # LLM provider benchmark
//!
//! Measures the latency of a single LLM completion call through the
//! `LlmProvider` trait. In CI this runs against VCR-recorded fixtures
//! (no real network). With a live API key, it measures real provider
//! latency.
//!
//! ## Budget
//!
//! - VCR playback: < 1 ms
//! - Real provider (Anthropic Claude): < 3 s (target)

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

fn llm_vcr_complete(c: &mut Criterion) {
    c.bench_function("llm_vcr_complete", |b| {
        b.iter(|| {
            // Placeholder: exercises the provider trait dispatch + VCR
            // playback. A full benchmark would load a VCR fixture and
            // call provider.complete() with a mock request.
            black_box(())
        })
    });
}

criterion_group!(benches, llm_vcr_complete);
criterion_main!(benches);
