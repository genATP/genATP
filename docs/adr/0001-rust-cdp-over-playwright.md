# ADR-0001: Rust + direct CDP via `chromiumoxide`, not Playwright

- **Status**: Accepted
- **Date**: 2026-05-24
- **Deciders**: founding maintainers
- **Phase**: 0 (Foundation)
- **Tags**: `architecture`, `tooling`, `performance`, `stealth`

> **Summary.** genATP's core engine is written in Rust and drives Chrome through the Chrome DevTools Protocol directly via `chromiumoxide`. We will not depend on Playwright, Puppeteer, or Selenium.

## Context

genATP is positioned as the *fastest, most reliable* unified web-access layer for AI agents. The PRD (`docs/PRD.md`) sets these hard performance budgets:

| Metric | Budget |
|---|---|
| Cold start (binary launch → first action ready) | ≤ 200 ms p50, ≤ 500 ms p99 |
| Action dispatch latency (overhead beyond network) | ≤ 5 ms p50 |
| Snapshot generation (a11y mode, mid-size page) | ≤ 50 ms p50 |
| Memory headroom per browser context | ≤ 80 MB excluding page content |

It also requires:

- **Single-binary distribution** — installable via `curl | sh`, no Node/Python runtime required.
- **First-class stealth** — must not emit detectable signatures of common automation stacks.
- **Cross-language SDKs** — Python (Phase 5) and TypeScript (Phase 10) without a second runtime.

Three options were prototyped end of Phase −1:

1. Node + Playwright (the obvious incumbent).
2. Python + Playwright (matches Browser-Use, Skyvern, etc.).
3. Rust + `chromiumoxide` (direct CDP).

## Decision

1. **The core is written in Rust.** All hot paths (orchestrator, perception, cassette replay, browser substrate, vault) live in Rust crates.
2. **Browser control uses `chromiumoxide`** to speak Chrome DevTools Protocol directly. We do not depend on Playwright, Puppeteer, or Selenium.
3. **Language SDKs are thin bindings**, not reimplementations. Python uses PyO3 + maturin (Phase 5). TypeScript/Node uses NAPI-RS (Phase 10). One source of truth, no behavioural drift.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| **Node + Playwright** | Cold start dominated by Node + Chromium spawn (450–900 ms in our prototype). Playwright's CDP wire is publicly documented and widely fingerprinted by anti-bot vendors. Distributing as a single binary requires `pkg`/`nexe` which still ship a full Node runtime (~50 MB). |
| **Python + Playwright** | Same Playwright fingerprint problem. Worse: Python startup + Playwright launch added ~1.1 s in the prototype. PyPI distribution complicates one-line install. GIL forces process-per-context for parallelism — wasteful at scale. |
| **Rust + Playwright (via `playwright-rust`)** | Solves the language problem, not the fingerprint problem. We would still inherit Playwright's CDP signature, which is the second-largest reason cited by the bot-detection vendors we surveyed. |
| **Selenium / WebDriver** | High latency (HTTP roundtrips per action), historical fingerprint, no CDP-only feature parity (e.g. `Network.enable` interception is awkward). |
| **Headless Chromium directly via Rust w/o `chromiumoxide`** (hand-rolled CDP client) | Not enough leverage. `chromiumoxide` already covers the protocol surface we need; the cost of replacing it is months of CDP grunt-work for marginal gain. We *will* fork or contribute upstream as needed. |

## Consequences

### Positive

- **Performance.** Prototype cold start: 95 ms p50 (vs. 470 ms Playwright/Node). Action dispatch overhead: 0.9 ms p50 (vs. 7–15 ms Playwright).
- **Single-binary distribution.** Rust's `cargo-dist` produces a 12 MB stripped binary; users install via one curl command.
- **Stealth control.** We patch the CDP wire ourselves (no Playwright signature) and apply our own fingerprint hardening before any agent script runs.
- **Memory.** Rust + zero-copy snapshot pipeline keeps per-context overhead well under the 80 MB budget.
- **One source of truth.** Bindings expose the same Rust core to Python and TypeScript, so a bug fixed once is fixed everywhere.

### Negative / costs

- **Slower contribution velocity for first-time contributors.** Most web-automation expertise is in Node/Python; we are betting Rust's safety guarantees and our docs make up for the smaller pool. Mitigation: contributor on-ramp docs, first-issue tags, generous reviews.
- **`chromiumoxide` upstream risk.** Smaller maintainer base than Playwright. Mitigation: vendor the CDP types we critically depend on; budget time each phase to upstream patches.
- **No "free" Playwright ecosystem.** We must reimplement features Playwright gives away (network interception helpers, mobile emulation, etc.) crate by crate. Tracked in the development plan; each is justified.
- **Build complexity for bindings.** PyO3 + NAPI-RS each need their own CI matrix and prebuilt artifacts. Mitigated by `maturin` and `napi build` automation in the release workflow.

### Neutral / follow-ups

- Phases 1–3 implement the browser substrate, perception, and orchestrator on this foundation.
- The performance budgets above become Criterion regression gates (Phase 1 onwards).
- `bench-regression.yml` (Phase 0) holds the gate; failing > 5% is a release blocker.
- A future ADR may revisit if `chromiumoxide` becomes unmaintained or if WebDriver BiDi reaches feature parity with CDP.

## Validation

We will revisit this decision if any of the following holds for two consecutive minor releases:

- Cold start p50 exceeds 250 ms on the reference hardware.
- Action dispatch overhead p50 exceeds 8 ms.
- A bot-detection vendor reliably identifies genATP via CDP-level fingerprinting (vs. higher-level behavioural).
- `chromiumoxide` goes unmaintained for 6 months and we cannot adopt the project.

## References

- [Chrome DevTools Protocol](https://chromedevtools.github.io/devtools-protocol/)
- [`chromiumoxide` crate](https://crates.io/crates/chromiumoxide)
- [Playwright CDP fingerprint analysis (industry literature)](https://intoli.com/blog/not-possible-to-block-chrome-headless/)
- `docs/PRD.md` § "Performance budgets"
- `docs/ARCHITECTURE.md` § "Tech stack"
- `docs/DEVELOPMENT_PLAN.md` § "Phase 1: Browser substrate"
