# genATP — Product Requirements Document

**Generative Agent Test Protocol**
*The unified web-access and replay layer for AI agents.*

| | |
|---|---|
| **Status** | Draft v1.0 |
| **Owner** | Founding team |
| **Last updated** | 2026-05-22 |
| **Engineering plan** | [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md) |
| **Target launch** | v0.1 Personal release — 8 weeks · v1.0 GA — 40 weeks (~10 months) |

---

## 1. TL;DR

Every AI agent that touches the web today is duct-taped together from six vendors (a framework, a cloud browser, an MCP server, a search API, a CAPTCHA solver, a proxy). It burns 100k+ tokens per task, breaks on every login wall, and fails the moment a site re-skins. **genATP** is one tool — `pip install genatp` / `npm i genatp` / drop-in MCP server — that replaces the entire stack and adds two capabilities no one ships today:

- **Action Cassettes** — record-once, replay-forever workflows that run with **zero LLM calls** and self-heal when the page changes.
- **ATP Spec** — a declarative YAML/JSON format for agentic web tasks that doubles as a test, an automation, and a production runbook.

If we ship this well, every developer building an AI agent that touches the web has a reason to install genATP within twelve months.

---

## 2. The Problem

### 2.1 The translation gap
The web (≈1.9B sites) was built for human eyes and hands. Agents are software. The friction is structural:

| Pain | Cost today |
|---|---|
| Raw HTML poisoning context | 50k–200k tokens/page, $0.50–$2.00/page |
| Selector rot (`.btn-primary-v3_final`) | Scripts written Monday break Wednesday |
| Login walls / MFA | Re-auth every run; humans-in-the-loop |
| Anti-bot detection (Cloudflare, DataDome) | 89% benchmark → 40–60% real-world success |
| Vendor fragmentation | 6 accounts, 6 bills, 6 things that break |
| Dynamic DOM (SPAs, shadow DOM, iframes) | Snapshots stale before agent acts |

### 2.2 The token economics
A 20-step Playwright-MCP workflow burns ~2.28M tokens (≈$3+ per run for what a human does in 30 seconds). The "Playwright MCP burns 114k tokens per test" complaint is now the canonical pain in the space.

### 2.3 The market gap
No tool today combines all of: Python + TypeScript SDK, built-in MCP server, encrypted auth vault, token-optimized diff snapshots, automatic DOM↔vision switching, stealth, fully self-hosted, AND deterministic replay. We will be the first.

---

## 3. Vision & Positioning

> **One install. Any agent. Any LLM. Any site. 90% fewer tokens.**

genATP is the missing universal abstraction layer between AI agents and the web. We are to AI agents what Playwright was to QA automation, what Postman was to REST APIs, and what npm was to JavaScript — combined.

**Positioning statement:** *For developers building AI agents that interact with the web, genATP is the unified web-access protocol that replaces the six-vendor stack with one install, cuts token cost by 10×, and lets agents replay learned workflows deterministically — unlike browser-use, Stagehand, or Playwright MCP, which solve only one slice each.*

### 3.1 Strategic moats

1. **Action Cassettes (defensible moat).** Once a developer's agent has a library of cassettes, switching costs are real — those cassettes encode their team's institutional knowledge of how to use every internal and external tool.
2. **Cassette Registry network effect.** A signed, versioned, community-curated marketplace of web actions. Every contribution makes the next user's agent smarter for free.
3. **Source-available, commercial-paid.** Free for personal and non-commercial use builds the developer community and credibility; any commercial use (companies, paid products, revenue-generating services) requires a paid license — managed Cloud and Enterprise tiers monetize commercial demand directly. See [`../LICENSE`](../LICENSE).

---

## 4. Personas

### P1 — Maya, the solo agent builder
- Building a research agent in Python on weekends.
- Spends 40% of build time fighting Playwright, MCP config, login bugs.
- Wants: one `pip install`, agent works in 10 lines, login persists.
- Win condition: she tweets the quickstart.

### P2 — Devansh, the seed-stage AI startup CTO
- 4-person team shipping an AI sales-research product.
- Browserbase ($39/mo min) + Tavily ($50/mo) + custom auth scripts.
- Wants: predictable cost, self-hosted dev, cloud-managed prod, SOC 2 path.
- Win condition: $400/month bill drops to $40, 3× faster dev cycles.

### P3 — Lena, the ML platform engineer at a Fortune 500
- Building internal "agent that does the boring browser work" for ops.
- Cannot send data to a third-party cloud. Needs audit logs, SSO, on-prem.
- Wants: enterprise license, SOC 2 report, signed cassettes for compliance.
- Win condition: passes security review in 2 weeks instead of 2 quarters.

### P4 — Tomás, the QA lead modernizing a test suite
- 5,000 brittle Playwright tests rewritten yearly because of CSS class churn.
- Wants: AI-generated, self-healing tests via ATP Spec.
- Win condition: flakiness drops below 1%, no Selenium upgrade ever again.

---

## 5. Goals & Non-Goals

### 5.1 Goals (v1.0)
- ✅ Single install (`pip install genatp` / `npm i genatp` / `brew install genatp`).
- ✅ MCP server, Python SDK, and TypeScript SDK from day one — same engine.
- ✅ Token cost ≤ 1/10th of Playwright MCP on the WebVoyager subset.
- ✅ Persistent encrypted auth vault, OS-keychain-backed.
- ✅ Automatic DOM-first → vision-fallback perception.
- ✅ Record + replay (Action Cassettes) with self-healing.
- ✅ Declarative ATP Spec format (`.atp.yml`) — versioned, signed.
- ✅ Top-tier baseline anti-bot (stealth user-agent, fingerprint, mouse cadence).
- ✅ ≥ 85% WebVoyager success at < 35k tokens per task average.

### 5.2 Non-goals (v1.0)
- ❌ Building a hosted LLM (we are LLM-agnostic).
- ❌ Becoming a proxy/residential-IP vendor (we partner, we don't operate).
- ❌ Beating enterprise WAFs like PerimeterX out-of-the-box (cloud tier).
- ❌ A drag-and-drop no-code builder (post-1.0).
- ❌ Mobile app automation (post-1.0).

---

## 6. Product Surface

### 6.1 Core capabilities

| # | Capability | Description | Differentiator |
|---|---|---|---|
| C1 | **Universal interface** | MCP server + Python SDK + TS SDK over one engine | Only tool with all three from day one |
| C2 | **Optimized snapshots** | Accessibility tree → compressed refs (`@e1`) → diff-only updates | 90% fewer tokens vs Playwright MCP |
| C3 | **Auto-perception switch** | DOM-first; auto-fallback to screenshot+vision when tree is sparse | Invisible to agent; cost-optimal |
| C4 | **Auth Vault** | Encrypted per-domain session store; OS keychain + biometric unlock | No OSS competitor ships this |
| C5 | **Action Cassettes** | Record successful runs as deterministic, signed, versioned replays | **Net-new category** |
| C6 | **ATP Spec** | YAML/JSON declarative format for agent web tasks | **Net-new category** |
| C7 | **Cassette Registry** | `atp install github/login` — community marketplace | **Net-new category** |
| C8 | **Stealth engine** | Fingerprint randomization, human-cadence input, TLS spoofing | Best-in-class self-hosted baseline |
| C9 | **Budget guardrails** | Per-task token and time budgets with abort/ask hooks | Critical for production |
| C10 | **Time-travel debugger** | Every snapshot is content-addressed; replay any step | Investigative superpower |

### 6.2 The ATP Spec (illustrative)

```yaml
# checkout.atp.yml
version: 1
name: "Stripe checkout — annual plan"
auth: stripe.com           # uses vault
budget:
  tokens: 8000
  seconds: 30
steps:
  - navigate: https://app.stripe.com/billing
  - click: "Upgrade plan"
  - select:
      label: "Annual"
  - fill:
      label: "Card number"
      from: secret://stripe.card
  - assert: "Subscription active"
on_change:
  strategy: heal           # invoke LLM if DOM changes
  notify: slack://alerts
```

The same file is:
- A **test** (run in CI, fails on assertion).
- An **automation** (run on a schedule).
- A **production agent tool** (`agent.run("checkout.atp.yml")`).

### 6.3 The 5-line quickstart we must hit

```python
from genatp import Agent

agent = Agent(model="claude-sonnet-4.5")
result = agent.run("Find the cheapest direct flight from SFO to JFK next Friday.")
print(result.summary)
```

If a developer cannot get from `pip install` to a working result in under 90 seconds, we have failed at the marketing layer.

---

## 7. Architecture Snapshot

Five layers, one binary:

1. **Interface** — MCP server, Python SDK, TS SDK, CLI.
2. **Orchestrator** — Plans, budgets, retries, perception routing.
3. **Perception** — DOM snapshotter, vision fallback, diff engine.
4. **Browser substrate** — CDP-native (`chromiumoxide`), stealth-patched. No Playwright dependency.
5. **State** — Auth vault, cassette store, snapshot history (content-addressed).

(Full diagrams in [`ARCHITECTURE.md`](./ARCHITECTURE.md). Implementation sequence in [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md).)

---

## 8. Success Metrics

### 8.1 Adoption (north-star: weekly active agents)
| Milestone | Target | When |
|---|---|---|
| GitHub stars | 5,000 | Month 3 |
| GitHub stars | 25,000 | Month 12 |
| Weekly active installs | 1,000 | Month 3 |
| Weekly active installs | 20,000 | Month 12 |
| MCP installs (Claude Desktop, Cursor, etc.) | 10,000 | Month 6 |

### 8.2 Technical
| Metric | Target |
|---|---|
| WebVoyager success rate | ≥ 85% |
| Avg tokens per 20-step task | ≤ 35,000 (vs Playwright MCP ~2.28M) |
| Cassette replay LLM-call rate | 0 calls when DOM unchanged |
| Cassette self-heal success | ≥ 80% of structural changes |
| Cold-start latency (CLI → first snapshot) | < 2 seconds |

### 8.3 Revenue (post-month-6)
| Tier | Target ARR by month 18 |
|---|---|
| Cloud (Pro, $49/mo) | $600k |
| Enterprise ($25k–$100k) | $1.5M |
| Marketplace fees (10% of paid cassettes) | $50k |

---

## 9. Roadmap

Phase-by-phase build details and acceptance gates live in [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md). Summary by release tag below.

### v0.1.0 — "MCP works" (Week 8)
- MCP server (stdio + HTTP) with `navigate / snapshot / click / fill / extract / scroll / screenshot / get_url / go_back`.
- Token-optimized snapshots (diff + ref compression, BLAKE3 CAS).
- LLM orchestrator + budget guard.
- WebVoyager **subset** benchmark (≥ 70% target) published with reproducible methodology.

### v0.2.0 — Python SDK (Week 10)
- `pip install genatp` on all 5 platforms.
- PyO3 native bindings, async API, LangChain + LlamaIndex adapters.

### v0.3.0 — Auth Vault (Week 12)
- Encrypted per-domain session store; OS keychain–wrapped KEK.
- Capability-scoped grants.

### v0.5.0 — Cassettes (Week 16)
- Action Cassettes: record, replay, self-heal, hash-chain verification.

### v0.5.1 — ATP Spec (Week 17)
- Declarative `.atp.yml` parser + runner; JUnit XML for CI.

### v0.6.0 — TypeScript SDK (Week 19)
- `npm install genatp` at parity with Python; Node/Bun/Deno.

### v0.7.0 — Stealth engine (Week 21)
- Runtime CDP overrides, fingerprint pool, human-cadence input.
- Passes bot.sannysoft 100% / botd ≥ 90% / consistent Cloudflare and DataDome clearance on test sites.

### v0.7.1 — DOM ↔ Vision auto-switch (Week 22)
- Vision adapter; perception policy engine.

### v0.8.0 — Cassette Registry + Time-travel debugger (Week 26)
- OCI Distribution v1.1 registry; signed cassettes; trust ladder.
- Frame-by-frame replay UI for any past run.

### Cloud beta — Managed service (Week 30)
- Multi-tenant Kubernetes orchestrator, HSM-backed vault, residential-proxy integration, Stripe billing.

### Enterprise preview (Week 36)
- SOC 2 Type I, SAML/OIDC SSO, audit-log SIEM exporter, on-prem Helm chart.

### v1.0.0 — GA (Week 40)
- Full WebVoyager benchmark (586 tasks, ≥ 85% success, ≤ 35k tokens avg) published with reproducible methodology repo.

### v1.x — Network effect (post-GA)
- Public Cassette Marketplace with revenue share.
- A2A integration for multi-agent shared sessions.
- WebMCP bridge (consume + publish).
- Patched Chromium binary (stretch from v0.7) hardened and shipped by default.

---

## 10. Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Browser-use ships an MCP server first | Medium | Medium | Beat them on auth vault + cassettes; embrace their users via importer |
| Playwright MCP fixes token bloat | Medium | High | Cassettes are still net-new; pivot positioning to replay layer |
| Anti-bot vendors fingerprint our stealth defaults | High | Medium | Modular stealth plugin system; commercial proxy partnerships |
| WebMCP spec eats the MCP-server market | Medium | Medium | Be the best WebMCP client + server; cassettes work on top |
| Cassette security (malicious community uploads) | Medium | High | Mandatory signing, sandboxed execution, vault access requires explicit consent |
| LLM providers ship native browser tools | High | Medium | Be the layer they call; partner, don't compete |

---

## 11. Resolved decisions (see DEVELOPMENT_PLAN.md)

| Question | Decision |
|---|---|
| Browser substrate | **CDP-native from day one** via `chromiumoxide` — not Playwright |
| Cassette format | **JSON (JCS) + zstd + ed25519** |
| Default LLM in CLI | **None** — user supplies API key via `genatp init` → OS keychain |
| Registry topology | **Central npm-style** for v1; federated mirrors post-v2 |

---

## 12. Appendix: Competitive Matrix

| Feature | browser-use | Stagehand | Playwright MCP | Skyvern | **genATP** |
|---|:-:|:-:|:-:|:-:|:-:|
| Python SDK | ✅ | ❌ | ❌ | ✅ | ✅ |
| TypeScript SDK | ❌ | ✅ | ✅ | ❌ | ✅ |
| MCP server built-in | ❌ | ❌ | ✅ | ❌ | ✅ |
| Encrypted auth vault | ❌ | ❌ | ❌ | ❌ | ✅ |
| Token-optimized snapshots | partial | partial | ❌ | n/a | ✅ |
| DOM ↔ vision auto-switch | ❌ | ❌ | ❌ | ❌ | ✅ |
| Stealth (OSS) | ❌ | ❌ | ❌ | partial | ✅ |
| Self-hosted | ✅ | partial | ✅ | ✅ | ✅ |
| **Deterministic replay (Cassettes)** | ❌ | ❌ | ❌ | ❌ | **✅** |
| **Declarative spec (ATP)** | ❌ | ❌ | ❌ | ❌ | **✅** |
| **Community registry** | ❌ | ❌ | ❌ | ❌ | **✅** |
