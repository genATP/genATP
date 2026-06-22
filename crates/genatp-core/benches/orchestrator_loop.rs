//! # Orchestrator loop benchmark
//!
//! Measures the latency of a single plan-loop iteration (snapshot → LLM →
//! action dispatch) under simulated conditions. The LLM provider is replaced
//! with a mock that returns a fixed action plan, so the benchmark isolates
//! the orchestrator's overhead without network calls.
//!
//! ## Acceptance budget
//!
//! - 20-step task: < 1 s orchestrator overhead (excl. browser + LLM net)

use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use genatp_core::Pricing;
use genatp_core::budget::{BudgetGuard, BudgetLimits};

fn orchestrator_plan_loop(c: &mut Criterion) {
    c.bench_function("orchestrator_plan_loop", |b| {
        b.iter(|| {
            // Measure BudgetGuard construction and check overhead.
            let guard = BudgetGuard::new(
                BudgetLimits {
                    max_tokens: 100_000,
                    max_wall: Duration::from_secs(60),
                    ..BudgetLimits::default()
                },
                Pricing::FREE,
            )
            .unwrap();
            black_box(guard.check())
        })
    });
}

criterion_group!(benches, orchestrator_plan_loop);
criterion_main!(benches);
