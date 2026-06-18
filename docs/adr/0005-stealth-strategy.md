# ADR-005: Stealth Strategy — Runtime CDP Overrides vs Patched Chromium

## Status
Accepted (Phase 11, v0.7.0)

## Context

Modern bot-detection services (Cloudflare, DataDome, Akamai, PerimeterX,
Human Security) fingerprint browser traffic at multiple layers:

1. **JavaScript API surface** — `navigator.webdriver`, WebGL vendor/renderer,
   `navigator.plugins`, `navigator.languages`, `screen` dimensions, etc.
2. **CDP (Chrome DevTools Protocol) signature** — the WebSocket upgrade headers,
   CDP command ordering, and presence of automation-oriented domains.
3. **Network / TLS stack** — TLS cipher suites, TCP timestamps, HTTP/2
   settings frames, and IP reputation.
4. **Behavioural** — mouse movement curves, typing cadence, scroll patterns,
   viewport resize patterns, and time-on-site distributions.

genATP needs a stealth strategy that delivers credible baseline evasion
while deferring the hardest problems (patched Chromium, residential proxy
integration) to later phases.

## Decision

**Phase 11 implements runtime CDP overrides only.** A patched Chromium
binary is deferred to Phase 14+ (v0.8.0+).

### What we do in Phase 11

| Layer | Approach | Mechanism |
|---|---|---|
| JavaScript API | Script injection via `Page.addScriptToEvaluateOnNewDocument` | JS IIFE that overrides `navigator.*`, `WebGLRenderingContext.getParameter`, `screen.*`, and `performance.now()` before page code runs |
| CDP emulation | `Emulation.setTimezoneOverride` + viewport configuration | CDP-native overrides for timezone, locale, and screen metrics |
| Input simulation | Bezier mouse paths + gamma-distributed typing delays | Client-side path generation with real-time dispatch via CDP `Input.dispatchMouseEvent` / `Input.dispatchKeyEvent` |
| Profile rotation | Fingerprint pool (25+ profiles, round-robin or random) | Each page acquire selects a new profile from the pool; profile encodes all JavaScript-level overrides |

### What we do NOT do in Phase 11

| Layer | Why deferred | Target phase |
|---|---|---|
| Patched Chromium binary | Engineering cost of maintaining a Chromium fork is ~1–2 dev-months. Runtime overrides cover 80%+ of detection surface for the majority of sites at a fraction of the cost. | Phase 14 (v0.8.0) |
| TLS fingerprint spoofing | Requires `rustls` / BoringSSL patches or a proxy layer. Most TLS fingerprinting targets undetectable automation stacks (SeleniumGrid, Puppeteer), not direct CDP drivers. | Cloud tier (Phase 15) |
| Residential proxy integration | Capital-intensive; partner model preferred. | Cloud tier (Phase 15) |

## Consequences

### Positive

1. **Single binary, no fork.** genATP continues shipping as a single static
   binary. Users do not need to install a custom Chromium build.
2. **Rapid iteration.** Runtime overrides can be tweaked, profile data
   updated, and new evasion vectors patched without a Chromium recompile.
3. **Backward compatible.** Existing installs get stealth for free on the
   next `genatp` binary update.
4. **Testable in CI.** The stealth test suite probes `bot.sannysoft.com`,
   `fingerprintjs.github.io`, and `creepjs` to measure regression; failures
   are quarantined to a nightly workflow so they don't block PRs.

### Negative

1. **Not undetectable against determined adversaries.** A site that runs
   behavioural analysis over multi-session interactions (e.g., Human
   Security) can still fingerprint the automation pattern. The patched
   Chromium path (Phase 14) addresses this at the cost of a fork.
2. **JavaScript injection is race-prone.** If a page script executes
   synchronously on DOMContentLoaded before our injected script runs,
   the override may not apply. `addScriptToEvaluateOnNewDocument` mitigates
   this by running before any page script, but `eval()` and dynamically
   created iframes remain a gap.
3. **Maintenance burden.** Profile data (WebGL renderer strings, plugin
   lists, font lists) must be refreshed every ~3 months as Chrome and
   GPU vendors update.

### Neutral

4. **AD-005 supersedes the Phase 11 stretch goal** of a patched Chromium
   build pipeline. The `fixtures/stealth/` adversarial suite explicitly
   tracks what runtime overrides can and cannot defeat, providing data
   for the Phase 14 patched-Chromium decision.

## Alternatives considered

### Alternative A: Patched Chromium only
Rejected because maintaining a Chromium fork for a 2-person team would
consume 50%+ of Phase 11 engineering time, leaving no room for the
fingerprint pool, input simulation, or test suite.

### Alternative B: Third-party stealth plugin (e.g., puppeteer-extra-stealth)
Rejected because these are Node.js-only, incur an IPC hop, and cannot
integrate with our CDP-native architecture without adding a Node process.

### Alternative C: Proxy-based evasions (TLS + IP)
Deferred to Cloud tier (Phase 15). These are capital-intensive (proxy
bandwidth costs) and better suited to a managed service with per-session
billing.

## Related

- [`docs/DEVELOPMENT_PLAN.md`](../DEVELOPMENT_PLAN.md) § Phase 11
- [`crates/genatp-browser/src/stealth.rs`](../../crates/genatp-browser/src/stealth.rs)
- [`crates/genatp-browser/src/input.rs`](../../crates/genatp-browser/src/input.rs)
- [`.github/workflows/nightly-stealth.yml`](../../.github/workflows/nightly-stealth.yml)
