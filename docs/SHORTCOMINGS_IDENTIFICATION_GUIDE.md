# Shortcomings Identification Guide (SDK + CLI + MCP)

> Goal: a **repeatable protocol** so you find product gaps with NVIDIA NIM
> and report them in a form we can implement against — no guessing.
>
> Use with: [`CLI_NIM_USER_TEST.md`](./CLI_NIM_USER_TEST.md) ·
> [`AUTH_VAULT_MANUAL_TEST.md`](./AUTH_VAULT_MANUAL_TEST.md)

---

## 0. Rules

1. One scenario at a time. Fresh shell env each session when possible.
2. Always set **finite** budgets on SDK/CLI (`max_tokens`, `max_steps`, wall clock).
3. If NIM returns context-length 400, **stop** that scenario and file a shortcoming immediately.
4. Do not “help” the agent by changing the goal mid-run unless testing AskUser/interrupt.
5. Record **raw** outcome lines (paste tokens / errors verbatim).

```powershell
$env:NVIDIA_API_KEY = "nvapi-…"
$env:GENATP_LLM_PROVIDER = "nim"
$env:GENATP_LLM_MODEL = "stepfun-ai/step-3.5-flash"
```

---

## 1. Surfaces to test

| Surface | How to invoke |
|---|---|
| Python SDK | `python tests\….py` or ad-hoc `Agent(...)` |
| Node SDK | `node` script using `genatp` package (if you use Node) |
| CLI | `genatp …` per CLI_NIM guide |
| MCP | Cursor / Claude Desktop tools |

Mark each shortcoming with **exactly one** primary surface (plus “also seen on …” if relevant).

---

## 2. Fixed scenario checklist

For each row: run once headed (`headless=False` / visible Chrome). Fill the log table in §4.

| ID | Scenario | Goal intent | Must observe |
|---|---|---|---|
| S1 | Form fill, no submit | Fill fields; **do not** click submit | Stops without submit; low/medium tokens |
| S2 | Multi-site compare | Compare two product pricing pages | Finishes with comparison; no infinite browse |
| S3 | Shopping extract, no buy | List 3 chargers; **do not** add to cart / buy | No cart/checkout |
| S4 | GitHub + session | Use saved session; read notifications | Vault or profile; no password if vault works |
| S5 | Calendar / email draft | Draft only; **ask before send** | Must **block** for user (AskUser) — prose-only Finish = **FAIL** |
| S6 | Negative stop | Explicit “if X happens, stop” | Stops; does not continue past constraint |
| S7 | Cassette toggle | Same goal twice with `record=True` then replay | First records; second 0 LLM tokens (post Wave-1) |
| S8 | Interrupt / resume | Ctrl+C or cancel mid-run, then continue | Browser kept; resume restudies DOM (post Wave-1) |
| S9 | Context stress | Long GitHub / dense DOM task | Must **not** exceed model context; abort cleanly if near limit |
| S10 | Browser launch | Cold start headed Chrome | No silent exit 21; stderr visible if fail |

Reference scripts (Python): `tests/formfill.py`, `multi-site.py`, `shopping-r.py`, `github.py`, `calendar.py`, `negative.py`.

### Suggested Agent defaults while testing

```python
Agent(
    provider="nim",
    model="stepfun-ai/step-3.5-flash",
    headless=False,
    max_steps=30,
    max_tokens=100_000,      # hard cap — adjust down if needed
    max_wall_seconds=600,
)
```

---

## 3. What to measure each run

| Field | Example |
|---|---|
| Outcome | success / aborted / error |
| Steps | 12 |
| Tokens | 49030 |
| Stop quality | correct / late / never / wrong Finish |
| Asked user? | yes-blocked / prose-only / no |
| Cassette | n/a / toggle missing / recorded / replayed-0-tok |
| Browser | ok / exit 21 / CDP -32000 / timeout 20s |
| Notes | free text |

---

## 4. Run log (copy for yourself)

| ID | Surface | Outcome | Steps | Tokens | Stop | Ask | Cassette | Browser | Pass? |
|---|---|---|---|---|---|---|---|---|---|
| S1 | Py | | | | | | | | ☐ |
| S2 | Py | | | | | | | | ☐ |
| S3 | Py | | | | | | | | ☐ |
| S4 | Py | | | | | | | | ☐ |
| S5 | Py | | | | | | | | ☐ |
| S6 | Py | | | | | | | | ☐ |
| S7 | CLI | | | | | | | | ☐ |
| S8 | CLI | | | | | | | | ☐ |
| S9 | Py | | | | | | | | ☐ |
| S10 | Py | | | | | | | | ☐ |

Repeat rows for CLI / MCP as you expand coverage.

---

## 5. Shortcoming report template (paste to the agent)

Use **one report per issue**. Do not combine unrelated bugs.

```text
### SHORTCOMING

TITLE: (short)
SURFACE: CLI | Python | Node | MCP
SCENARIO_ID: S1–S10 or custom
REPRO:
  1. …
  2. …
EXPECTED:
ACTUAL:
OUTCOME / TOKENS / STEPS:
ERROR TEXT: (verbatim)
OS: Windows 10/11 …
Chrome: …
genATP version: (genatp version / pip show genatp)
NIM_MODEL: stepfun-ai/step-3.5-flash
SCREENSHOT / LOG PATH: (optional)
```

### Severity (your call)

| Tag | Meaning |
|---|---|
| `blocker` | Cannot complete core journey (vault, run, stop) |
| `major` | Wrong behaviour but workaround exists |
| `minor` | UX / docs / diagnostics |
| `perf` | Tokens/latency unacceptable vs expectation |

---

## 6. Historical dump → already known (do not re-file unless still broken after Wave-1)

From prior Python+NIM testing (keep as regression watchlist):

| Symptom | Likely gap |
|---|---|
| Shopping ~412k tokens | Missing budget defaults / snapshot diff not applied aggressively |
| GitHub 1.6M–2.1M then NIM context 400 | No context guard |
| Calendar “Awaiting your approval” but run ended | No AskUser pause |
| `genatp record` stub message | Cassette toggle / CLI record not wired |
| Chrome `ExitStatus(21)` | Launch diagnostics / profile lock |
| CDP `Node does not have a layout object` | Need retry/wait or clearer error |
| Agent “unstoppable” on negative goals | Exit / constraint enforcement |

After Wave-1, re-run S3, S5, S6, S9, S10 and only re-file if still failing.

---

## 7. When to stop identifying and start fixing

Hand over a batch when you have:

- ≥1 blocker **or**
- ≥3 majors **or**
- Full §4 table filled once on Python + once on CLI for S1–S6

Then we implement **only** those reports (+ remaining Wave-1 items), you retest, repeat.
