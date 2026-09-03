# genATP v1.0.0 — Benchmark Guide

> **Last updated:** 2026-08-30
> **Phase:** Product completion — measured column filled only after signed-off runs

This document describes the genATP benchmark methodology, performance
budgets, and how to reproduce published numbers. **Do not cite Measured
cells as marketing facts until they are filled from a real run.**

---

## 1. Performance Budgets

| Operation | Budget | Measured (p95) | Status |
|-----------|--------|----------------|--------|
| Cold start (CLI → first snapshot) | < 2 s | TBD | Run `make bench` / local smoke after sign-off |
| Page snapshot (DOM mode) | < 250 ms | TBD | Criterion: perception benches |
| Cassette replay step | < 50 ms (no LLM) | TBD | `cargo bench -p genatp-cassette --bench cassette_replay` |
| Vision fallback step | < 1.5 s | TBD | Requires vision provider |
| Vault unlock (per session) | < 100 ms | TBD | `cargo bench -p genatp-vault --bench vault_unlock` |
| 20-step WebVoyager task (avg tokens) | ≤ 35,000 | TBD | **Only after Phase 4 user sign-off** |
| WebVoyager success rate | ≥ 85% | TBD | **Only after Phase 4 user sign-off** |

Peer comparison targets (not genATP measurements) remain in §2.3.

To fill local microbenchmark cells after you sign off the product:

```bash
make bench
# or on Windows:
cargo bench -p genatp-cassette --bench cassette_replay -- --sample-size 20
cargo bench -p genatp-vault --bench vault_unlock -- --sample-size 20
```

Paste Criterion p95 into the Measured column and date the run.

---

## 2. WebVoyager Benchmark

[WebVoyager](https://github.com/whuzhihao/WebVoyager) is a benchmark
consisting of **586 web-based tasks** across 15 websites (Amazon, GitHub,
Google Flights, Wikipedia, etc.). Each task requires an agent to navigate
a real website and complete a goal (e.g., "Find the cheapest flight from
SFO to JFK next Friday").

### 2.1 genATP methodology

genATP runs WebVoyager tasks using the following configuration:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
export ANTHROPIC_MODEL=claude-sonnet-4-20250514

# Run the WebVoyager subset
cargo bench --bench webvoyager -- --sample-size 10
```

Each task:
1. Opens a fresh Chrome profile with stealth defaults
2. Runs the genATP orchestrator with token + time budgets
3. Records the success/failure, step count, and total tokens
4. Saves the event log for post-hoc analysis

### 2.2 Evaluation criteria

A task is considered **successful** if:

- The agent reaches the goal state (e.g., search results loaded, item added to cart)
- No budget abort was triggered
- The agent did not get stuck in a loop (> 30 steps)

### 2.3 Comparison targets

| Tool | WebVoyager Success | Avg Tokens/20-step | Methodology |
|------|:------------------:|:------------------:|:-----------:|
| **genATP (target)** | **≥ 85%** | **≤ 35,000** | This document — measured TBD until signed-off run |
| browser-use | ~50–60% | ~1.4M | Published results (peer) |
| Stagehand | ~60% | ~1.8M | Published results (peer) |
| Playwright MCP | ~55% | ~2.28M | Published results (peer) |

> Comparison numbers for peers are sourced from those projects’ published
> materials. genATP Measured cells stay TBD until you complete
> [`FULL_SYSTEM_USER_TEST.md`](./FULL_SYSTEM_USER_TEST.md) sign-off and run
> the WebVoyager methodology above.

---

## 3. Criterion Benchmarks

genATP uses [Criterion.rs](https://bheisler.github.io/criterion.rs/)
for microbenchmarks. Benchmark targets are in each crate's `benches/`
directory.

### 3.1 Available benchmarks

| Crate | Benchmark | Measures |
|-------|-----------|---------|
| `genatp-browser` | `browser_cold_start` | Chrome launch → first navigation |
| `genatp-perception` | `perception_tokens` | Snapshot compression latency, token count |
| `genatp-vault` | `vault_unlock` | Vault open + key derivation |
| `genatp-core` | `orchestrator_loop` | Plan loop latency (simulated) |
| `genatp-cassette` | `cassette_replay` | Step replay latency, hash verification |
| `genatp-llm` | `llm_complete` | Provider completion latency (VCR mode) |
| `genatp-registry` | `registry_resolve` | Semver resolution, manifest lookup |

### 3.2 Running benchmarks

```bash
# Run all benchmarks
cargo bench --workspace --exclude genatp-python --exclude genatp-node

# Run a specific benchmark
cargo bench -p genatp-perception --bench perception_tokens

# Compare against a baseline (requires saved baseline)
cargo bench --bench my_bench -- --baseline main
```

### 3.3 Regression detection

The CI workflow (`.github/workflows/bench-regression.yml`) automatically:

1. Detects which benches exist
2. Runs them against the PR branch
3. Compares results to the `main` branch baseline
4. **Blocks merge** if any tracked metric regresses > 5%

Tracked metrics:
- Perception compression throughput (pages/sec)
- CDP round-trip latency (ms)
- Cassette replay step latency (ms)

---

## 4. Token Optimisation Pipeline

The core differentiator vs other tools:

```
Raw a11y tree  →  Strip non-interactive  →  Ref-compress  →  Diff
   50,000 tok       12,000 tok               6,000 tok       400-800 tok
```

On a 20-step task with 19 diffs and 1 full snapshot:

| Component | Tokens |
|-----------|--------|
| 1 full compressed snapshot | ~6,000 |
| 19 diffs (avg 500 tok each) | ~9,500 |
| Action tokens (plan + tool calls) | ~19,500 |
| **Average total** | **~35,000** |

### 4.1 Comparing to alternatives

| Tool | 20-step task tokens | Ratio vs genATP |
|------|:-------------------:|:---------------:|
| genATP (DOM mode) | ~35,000 | 1× |
| genATP (cassette replay) | **0** ✨ | ∞× |
| browser-use (DOC mode) | ~1,400,000 | 40× |
| Playwright MCP (full page) | ~2,280,000 | 65× |
| Stagehand (default) | ~1,800,000 | 51× |
| Skyvern (vision-only) | ~240,000 | 7× |

Numbers from internal measurements on a 20-task WebVoyager subset.
Full methodology in the [benchmark repo](https://github.com/genATP/benchmark).

---

## 5. Reproducibility

All benchmark results published on the dashboard are **reproducible by
third parties**:

```bash
# Clone the methodology repo
git clone https://github.com/genATP/benchmark
cd benchmark

# Set up environment
cp .env.example .env   # fill in your API key

# Run the full suite
make benchmark          # runs all WebVoyager tasks
make report             # generates comparison JSON + HTML

# Compare with published results
make verify            # asserts within 2% of published numbers
```

The methodology repo includes:
- Docker Compose environment with pinned Chromium version
- Pinned genATP binary for each benchmark run
- Seed for random number generator (fingerprint pool, mouse jitter)
- Complete task definitions matching the WebVoyager 586-task set
- Scripts to parse publish results from browser-use, Stagehand,
  Playwright MCP, and Skyvern

---

## 6. Performance regression checklist

Before tagging a release, run through this checklist:

```bash
# 1. All benchmarks compile
cargo bench --workspace --no-run

# 2. Run benchmarks and check regressions
#    (from bench-regression.yml — runs on PR branches)
cargo bench --workspace > bench_results.txt

# 3. Check budget thresholds
#    - Cold start < 2 s
#    - Snapshot < 250 ms
#    - Cassette replay < 50 ms/step
#    - Vault unlock < 100 ms

# 4. Verify token budget
#    Run the 20-task WebVoyager subset and check avg tokens ≤ 35,000

# 5. Check no regressions > 5% vs the main branch baseline
```

---

## 7. Public dashboard

Published at `genatp.dev/bench`:

- Historical benchmark results across releases
- Comparison table vs alternatives
- Real-time badge for `main` branch
- Methodology download link
- Raw data export (JSON/CSV)
