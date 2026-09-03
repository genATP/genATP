# Beating the Token Tax: How Accessibility Diffs Make Web Agents 10× Cheaper

*Raw HTML is a poison. Snapshots should shrink as the page changes — not explode.*

---

### Suggested Medium tags
`LLM` · `Web Agents` · `Accessibility` · `MCP` · `Cost Optimization`

---

If you have ever driven a browser through an LLM tool loop, you have felt the **token tax**.

The agent opens a page. The tool returns “the page.” That payload is often a giant accessibility tree or — worse — raw HTML. The model must read it before it can click anything. Then it clicks. Then it reads *another* full page. Twenty steps later, you have spent a small fortune to move a mouse.

This is not a model-quality problem. It is an **information architecture** problem.

**genATP** (Generative Agent Test Protocol) is built around one brutal design tenet:

> Tokens are the most expensive resource — optimize them ruthlessly.

This post explains *why* browser agents get expensive, *what* genATP’s perception pipeline does differently, and *how* DOM↔vision auto-switching keeps cost optimal without making the agent dumber.

---

## Where the tokens actually go

A browser agent’s bill is dominated by three things:

1. **Tool schema overhead** — MCP clients keep tool definitions in context.
2. **Page observation** — every `snapshot` / `navigate` return value.
3. **Planning chatter** — the model’s reasoning between actions.

Item (2) is the silent killer. Complex pages can return accessibility trees tens or hundreds of kilobytes large. Microsoft’s Playwright MCP issue tracker has reports of snapshots in the **50KB–540KB** range causing context overflows after only a few navigations ([playwright-mcp#1233](https://github.com/microsoft/playwright-mcp/issues/1233)). Independent measurements of full-tree MCP snapshots on dense pages have shown **tens of thousands of tokens per look**.

Mind2Web made the research case early: raw HTML from real websites is often too large to feed an LLM usefully; filtering is mandatory ([arXiv:2306.06070](https://arxiv.org/abs/2306.06070)). WebVoyager later pushed multimodal web agents and real-site benchmarks ([ACL 2024](https://aclanthology.org/2024.acl-long.371/)). The industry still mostly ships “send the whole observation every turn.”

That is the tax.

---

## genATP’s bet: compress, then diff

genATP’s perception path is intentionally staged:

```
Raw accessibility tree
        ↓ strip noise
   Stripped tree
        ↓ compress interactive elements to refs (@e1, @e2, …)
   Ref-compressed view
        ↓ send only what changed
   Diff update to the LLM
```

In approximate token terms (order-of-magnitude, mid-size page):

| Stage | Typical scale |
|---|---|
| Raw a11y tree | ~50,000 tokens |
| Stripped | ~12,000 tokens |
| Ref-compressed | ~6,000 tokens |
| **Diff per subsequent step** | **~400–800 tokens** |

On a 20-step task, diffs compound. That is how you get from “millions of tokens” territory toward **tens of thousands** — and why genATP’s published comparison table targets roughly:

| Stack | ~Tokens / 20-step task |
|---|---:|
| Playwright MCP (published comparisons) | ~2,280,000 |
| browser-use (published comparisons) | ~1,400,000 |
| **genATP (design target)** | **≤ 35,000** (~34k on the README example) |
| **genATP cassette replay** | **0 LLM tokens** |

Methodology and reproduction notes live in the project’s benchmark docs. Treat the large competitor numbers as *order-of-magnitude published baselines*; treat genATP’s ≤35k figure as the **budget the engine is engineered against**.

---

## Refs, not CSS selectors

Selectors like `.btn-primary-v3_final` rot. Accessibility-grounded refs age better *within a snapshot epoch*, and they give the model a crisp action language:

```text
click @e12
fill @e7 "Prasad"
```

This is the same broad idea popularized by accessibility-snapshot MCP browsers, pushed further with **compression + diffing** so the model does not re-ingest the entire tree after every micro-interaction.

Playwright’s own MCP docs emphasize snapshots over screenshots for cost and precision ([playwright.dev/mcp/snapshots](https://playwright.dev/mcp/snapshots)). genATP agrees — then asks the next question: *why send an unchanged sidebar 19 more times?*

---

## DOM first, vision only when the tree lies

Not every page has a useful accessibility tree. Canvas apps, hostile shadow DOM, and sparse trees defeat DOM-only agents.

genATP’s policy is cost-optimal by default:

- **DOM mode first** — on the order of ~600 tokens for a lean observation when the tree is healthy.
- **Vision fallback automatically** — screenshot + vision path (~12k tokens class) when the tree is sparse or unusable.
- **Invisible to the agent API** — your prompt does not choose the mode; the orchestrator does.

That matters. Teams that hard-code “always screenshot” waste money. Teams that hard-code “always DOM” fail silently on the pages that matter. Auto-switch is not a gimmick — it is a **pricing policy for perception**.

---

## Budgets: make cost a runtime feature

Cheap snapshots are necessary but not sufficient. Agents loop.

genATP’s orchestrator enforces **per-task budgets** (tokens, time, and optionally USD in ATP Spec). When a run exceeds policy, it aborts or asks — instead of quietly becoming a $40 surprise.

Example from a real ATP Spec in the repo:

```yaml
version: 1
name: "DuckDuckGo Search"
base_url: "https://duckduckgo.com"
budget:
  tokens: 3000
  seconds: 30
  usd: 0.05
steps:
  - navigate: "/"
  - fill: { label: "search", value: "Rust programming" }
  - press: "Enter"
  - assert: "Rust"
```

The same file is CI test, automation, and agent tool. Budget is part of the contract.

---

## Architecture that makes this possible

Perception is not a Python script bolted onto Playwright.

genATP’s core is **Rust**, driving Chrome through **CDP directly** via `chromiumoxide` — not Playwright — so snapshot generation, stealth controls, and SDK bindings share one engine (see ADR-0001 in the repo). Python and TypeScript SDKs are thin bindings over that core. The MCP server exposes the same tools Claude Desktop / Cursor already speak.

One engine. Three faces. Same token path.

---

## Practical advice if you are building agents today

1. **Measure one snapshot.** Print token counts for a single `browser_snapshot` on your heaviest page before you tune prompts.
2. **Prefer structured a11y over HTML and screenshots** for interaction.
3. **Diff or truncate** — never accumulate full trees forever in chat history.
4. **Separate explore from repeat** — record successful paths; replay without an LLM.
5. **Put a hard budget in the run config** — hope is not a cost model.

---

## Closing

Web agents will not become production infrastructure until observation gets as engineered as inference.

The token tax is not inevitable. It is what you pay when every step re-sends the internet to a language model.

genATP’s answer is simple to say and hard to implement well: **strip → compress → diff → fall back to vision only when the DOM fails — and replay with zero tokens when you already know the path.**

---

**Project:** [github.com/genatp/genatp](https://github.com/genatp/genatp)  
**Docs:** [BENCHMARK.md](https://github.com/genatp/genatp/blob/main/docs/BENCHMARK.md) · [ARCHITECTURE.md](https://github.com/genatp/genatp/blob/main/docs/ARCHITECTURE.md)  
**References:** [Mind2Web](https://arxiv.org/abs/2306.06070) · [WebVoyager](https://aclanthology.org/2024.acl-long.371/) · [Playwright MCP snapshots](https://playwright.dev/mcp/snapshots)
