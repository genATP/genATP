# Full System User Test (self-hosted)

> Sign-off runbook after Wave-1 gaps are fixed. Exercise **vault + cassette +
> CLI + Python + Node + MCP** from a user perspective (Windows first).
>
> Companion guides:
> - [`AUTH_VAULT_LIVE_E2E.md`](./AUTH_VAULT_LIVE_E2E.md) — MCP save/load across restart
> - [`AUTH_VAULT_MANUAL_TEST.md`](./AUTH_VAULT_MANUAL_TEST.md)
> - [`CLI_NIM_USER_TEST.md`](./CLI_NIM_USER_TEST.md)
> - [`USER_MANUAL.md`](./USER_MANUAL.md)
> - [`SHORTCOMINGS_IDENTIFICATION_GUIDE.md`](./SHORTCOMINGS_IDENTIFICATION_GUIDE.md)
>
> Competitive benchmarks ([`BENCHMARK.md`](./BENCHMARK.md)) come **only after**
> this runbook passes for your journeys.

---

## 0. Environment (Windows PowerShell)

```powershell
cd E:\genATP\genATP

cargo build --release -p genatp-cli
$env:Path = "E:\genATP\genATP\target\release;" + $env:Path

$env:NVIDIA_API_KEY = "nvapi-…"
$env:GENATP_LLM_PROVIDER = "nim"
$env:GENATP_LLM_MODEL = "openai/gpt-oss-20b"
$env:RUST_LOG = "warn"

# Isolated Chrome profile (recommended beside MCP)
# $env:GENATP_USER_DATA_DIR = "$env:TEMP\genatp-full-e2e\profile"

# Optional isolated vault
# $env:GENATP_VAULT_PATH = "$env:TEMP\genatp-full-e2e\vault.sqlite"

genatp version   # expect 1.0.0
```

Python SDK (`python -m maturin develop --release` in `bindings/python`) and
Node SDK (`npm install && npx napi build --platform --release` in `bindings/node`)
should report **1.0.0**.

---

## 1. Pass/fail matrix

| # | Surface | Scenario | Pass criteria |
|---|---|---|---|
| 1 | CLI | `version` / `vault path` / `cassette list` | Commands exit 0 |
| 2 | CLI + NIM | `genatp run --goal … --start-url https://example.com` | Outcome success or clear abort; tokens printed; **≤ max-tokens** |
| 3 | CLI | Same with `--record --slug e2e-example` | Cassette path printed; `genatp cassette inspect e2e-example` works |
| 3b | CLI | Re-run same goal/url without `--skip-cassette` | Router hit: `Outcome: success (cassette replay)`, Tokens: 0 |
| 4 | CLI headed | Goal that needs approval (`ask_user`) | Process **blocks** on stdin until answer; does not `finish` with “awaiting approval” prose alone |
| 5 | CLI | Ctrl+C mid-run | Aborts after current step; message about interrupt |
| 5b | SDK | Python/Node resume with same `user_data_dir` | Continue-style goal runs without exit 21 |
| 6 | MCP | Live vault E2E (§3 below) | No re-login after restart; revoke restores login wall |
| 7 | Python | `Agent(record=True, max_tokens=100_000).run(…)` | Success + cassette; defaults bound tokens |
| 8 | Python | `ask_user=` callback or headed stdin | Mid-run question answered via hook |
| 9 | Node | `new Agent({ record: true, maxTokens: 100000 }).run(…)` | Same as Python for record + budgets |
| 10 | Negative | Goal: “do not submit / do not buy” | Agent stops without completing forbidden action |
| 11 | Replay | `genatp cassette replay <slug> [--user-data-dir …]` | Deterministic path, 0 LLM tokens, exit 0 |
| 12 | Self-heal | Replay stale cassette after DOM change (or `--no-heal` control) | Heal patches step **or** honest fail documented |
| 13 | Registry | `registry publish` + `search` | Non-empty search / publishers list |

Fill **Actual** / **Tokens** / **Notes** as you run. Paste failures using the shortcoming template in [`SHORTCOMINGS_IDENTIFICATION_GUIDE.md`](./SHORTCOMINGS_IDENTIFICATION_GUIDE.md).

---

## 2. Suggested execution order

1. CLI smoke (version, vault, cassette list) — no NIM spend  
2. Short NIM CLI run on `example.com` with `--max-steps 15 --max-tokens 50000`  
3. Record + inspect cassette + router hit (3b)  
4. Vault MCP path — [`AUTH_VAULT_LIVE_E2E.md`](./AUTH_VAULT_LIVE_E2E.md)  
5. Python Agent one-liner with NIM  
6. Node Agent one-liner with NIM  
7. Headed ask_user / interrupt smoke (§4)  
8. Cassette replay beside MCP with `--user-data-dir`  
9. Registry seed (§5)  
10. Self-heal spot check (§6)  
11. Negative constraint scenario  

---

## 3. Vault live E2E (matrix row 6)

Follow [`AUTH_VAULT_LIVE_E2E.md`](./AUTH_VAULT_LIVE_E2E.md) end-to-end. Minimum pass:

- `save_auth` → `list_auth` shows domain  
- Kill MCP → restart → `load_auth` → no re-login  
- `genatp vault revoke` → login wall returns  

---

## 4. AskUser, interrupt, resume (matrix rows 4–5, 5b, 8)

### 4.1 AskUser (CLI headed)

```powershell
genatp run --headed `
  --goal "Go to example.com. Before extracting the H1, use ask_user to confirm you should proceed. Then finish with the H1 text only." `
  --start-url "https://example.com" `
  --user-data-dir "$env:TEMP\genatp-ask-user" `
  --max-steps 15
```

**Expected:** stdout shows a question; process **blocks** until you type an answer on stdin. After answer, run completes with H1 in summary — not a `finish` that says “waiting for approval” without calling `ask_user`.

### 4.2 Interrupt (CLI)

Start a longer headed run, press **Ctrl+C** once.

**Expected:** cooperative abort after current step; stderr/stdout mentions interrupt (not a hung Chrome burn).

### 4.3 Resume (Python)

```python
from genatp import Agent

profile = r"C:\Temp\genatp-interrupt-resume"
agent = Agent(provider="nim", model="openai/gpt-oss-20b", headless=False, user_data_dir=profile)
# Start run, Ctrl+C mid-way, then:
agent.run("Continue from the current page. Original goal: extract the H1 from example.com",
          start_url="https://example.com")
```

Node: `agent.continueAfterInterrupt(...)` — requires `userDataDir`.

---

## 5. Registry seed (matrix row 13)

After recording a cassette:

```powershell
$path = Join-Path $env:USERPROFILE ".genatp\cassettes\e2e-example\versions\1.0.0.cassette"
genatp registry publish $path local-dev
genatp registry search e2e
genatp registry publishers
```

**Expected:** search returns the slug; publishers lists `local-dev`.

---

## 6. Self-heal spot check (matrix row 12)

1. Record a cassette with a click step on a page you can change (local fixture or dev site).  
2. Change the target button id/label so the recorded ref drifts.  
3. Replay **without** `--no-heal`:

```powershell
genatp cassette replay my-slug --user-data-dir "$env:TEMP\genatp-heal-test"
```

**Pass options:**

- Heal path runs LLM once for the drifted step, replay completes, minor version bump — document tokens.  
- **Or** clear failure with actionable message — document as known limitation (file shortcoming if unexpected).

Control: same replay with `--no-heal` on CLI agent router path should skip heal (`genatp run …` with matching slug).

---

## 7. Token / context watch

- Default ceilings: **100k tokens**, **600s wall**, context truncate ~**120k** estimated tokens before each LLM call.  
- Verified NIM example.com H1: ~**1058** tokens plan, **0** on cassette replay.  
- If NIM returns context-length errors, lower `max_tokens` / steps and file a shortcoming.  

---

## 8. Sign-off

Product feels complete for self-hosted use when:

- [ ] Vault login persistence works across process restarts ([`AUTH_VAULT_LIVE_E2E.md`](./AUTH_VAULT_LIVE_E2E.md))  
- [ ] Cassette record + router hit works on CLI + at least one SDK  
- [ ] `ask_user` truly pauses; interrupt does not leave an unstoppable burn  
- [ ] Cassette replay works with `--user-data-dir` beside MCP  
- [ ] Token defaults prevent silent multi-million-token burns  
- [ ] CLI is usable as a normal tool for the journeys above  

**Then** proceed to competitive benchmarks. Do not fill marketing numbers without measured runs.
