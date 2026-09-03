# CLI + NVIDIA NIM — Full Product User Test Guide

> Goal: exercise genATP **like a normal CLI tool** with NVIDIA NIM, and
> know which commands are real vs stub so you do not waste runs.
>
> Config reference: [`CONFIGURATION.md`](./CONFIGURATION.md)

---

## 0. One-time setup (Windows PowerShell)

```powershell
cd E:\genATP\genATP

# Build CLI
cargo build --release -p genatp-cli
$env:Path = "E:\genATP\genATP\target\release;" + $env:Path

# NIM
$env:NVIDIA_API_KEY = "nvapi-…"
$env:GENATP_LLM_PROVIDER = "nim"
$env:GENATP_LLM_MODEL = "openai/gpt-oss-20b"   # verified tool-calling; see CONFIGURATION.md
$env:RUST_LOG = "warn"

# Optional
# $env:CHROME_PATH = "C:\Program Files\Google\Chrome\Application\chrome.exe"
# $env:GENATP_VAULT_PATH = "$env:TEMP\genatp-vault-test\vault.sqlite"

genatp version
```

**Expected:** `genatp 1.0.0` (or current workspace version).

**Recommended NIM model for agents:** `openai/gpt-oss-20b` (verified ~1058 tok on example.com H1).  
Do **not** use `stepfun-ai/step-3.5-flash` (EOL, HTTP 410) or models that emit JSON-as-text without tool calls (see CONFIGURATION.md).

---

## 1. Command matrix (honest status)

| Command | Purpose | Status |
|---|---|---|
| `genatp version` | Version | **Works** |
| `genatp serve --stdio` | MCP server | **Works** |
| `genatp serve --port 3000` | HTTP MCP | **Works** |
| `genatp vault path\|list\|revoke` | Auth vault | **Works** |
| `genatp cassette list\|inspect\|replay` | Cassette store | **Works** (`replay` supports `--user-data-dir`) |
| `genatp record <slug> <url> <goal>` | Record agent run → cassette | **Works** (real orchestrator + NIM) |
| `genatp run --goal … --start-url …` | Run goal; optional `--record` | **Works** |
| `genatp atp …` | Parse/validate ATP specs | **Works** |
| `genatp replay <run-id>` | Time-travel debugger | **Works** |
| `genatp registry search\|install\|…` | Local registry | **Works** |

Always run `genatp --help` and `genatp <cmd> --help` on your build.

Defaults for `run` / `record`: `--max-tokens 100000`, `--max-wall-s 600`, `--max-steps 30`. Pass `0` for unlimited tokens/wall. Headed + Ctrl+C cooperatively aborts; `ask_user` prompts on stdin.

---

## 2. Smoke order (do in this order)

### 2.1 Binary + Chrome

```powershell
genatp version
Test-Path "C:\Program Files\Google\Chrome\Application\chrome.exe"
```

If Chrome missing, set `CHROME_PATH` or install Chrome. Exit code **21** on launch usually means Chrome died before CDP websocket — see browser diagnostics in Wave-1 notes / shortcomings guide.

### 2.2 Vault

```powershell
genatp vault path
genatp vault list
```

Full vault procedure: [`AUTH_VAULT_MANUAL_TEST.md`](./AUTH_VAULT_MANUAL_TEST.md).

### 2.3 MCP serve (optional here)

```powershell
genatp serve --port 3000
# other terminal: curl http://127.0.0.1:3000/health   # if exposed
```

Ctrl+C stops the server.

### 2.4 Cassettes

```powershell
genatp cassette list
# genatp cassette inspect <slug> --version 1.0.0
# genatp cassette replay <slug>
```

### 2.5 ATP specs in repo

```powershell
genatp atp validate examples/atp-spec-ci/login.atp.yml
genatp atp validate examples/atp-spec-ci/search.atp.yml
```

### 2.6 Agent goals via CLI

```powershell
genatp run --goal "Open example.com and extract the H1" --start-url "https://example.com" --max-tokens 50000 --max-steps 20

# Record a cassette on success
genatp record example-h1 "https://example.com" "Open example.com and extract the H1" --headed

# Or run with --record
genatp run --goal "…" --start-url "https://example.com" --record --slug example-h1 --headed
```

Stdout prints `Outcome`, `Steps`, `Tokens`, `Summary`, and `Cassette:` path when recording.

---

## 3. Mirror your Python scenarios from CLI (checklist)

| Scenario | Python script (today) | CLI target |
|---|---|---|
| Rust version lookup | `tests/test_rust.py` / `test_rust.py` | `genatp run --goal "…"` |
| Multi-site compare | `tests/multi-site.py` | same |
| Shopping extract (no buy) | `tests/shopping-r.py` | same + negative constraint in goal |
| GitHub + session | `tests/github.py` | vault load + run |
| Form fill no submit | `tests/formfill.py` | same |
| Calendar + approval | `tests/calendar.py` | must **AskUser** (not prose Finish) after Wave-1 |

For each run record:

- Outcome (`success` / `aborted` / `error`)
- Steps
- Tokens (input+output if printed)
- Whether agent stopped when told “do not submit / do not buy”
- Browser errors (CDP, timeout, exit 21)

**Context overflow watch:** if tokens approach ~200k on NIM (model limit often 262144), **abort** and file a shortcoming — Wave-1 context guard must prevent silent 400s.

---

## 4. How to read results

| Signal | Meaning |
|---|---|
| `Outcome: success` | Orchestrator returned `Finish` or equivalent success |
| `Outcome: aborted` | Step/budget/wall limit (or interrupt) |
| `Outcome: error` | Browser/LLM/transport failure |
| NIM HTTP 400 `maximum context length` | Context guard should truncate; if still hit, lower `--max-tokens` / steps and file a shortcoming |
| Cassette path printed / `cassette inspect` | Record path works |

---

## 5. Known time-wasters

1. Leaving `max_tokens=0` (unlimited) on long NIM runs — default is now 100k; pass `0` only intentionally.
2. Reusing a locked Chrome user-data-dir while Chrome is already open (launch exit 21).
3. Expecting Cloudflare-hard sites to always work with default stealth — treat stealth as **best-effort**.

---

## 6. Sign-off table

| # | Check | Pass? |
|---|---|---|
| 1 | `genatp version` works with NIM env set | ☐ |
| 2 | Vault path/list work | ☐ |
| 3 | `serve` starts and stops cleanly | ☐ |
| 4 | At least one agent goal completes under token budget | ☐ |
| 5 | Negative goal (“do not buy”) does not purchase | ☐ |
| 6 | Record writes a cassette | ☐ |
| 7 | Replay / router hit uses 0 LLM tokens | ☐ |
| 8 | AskUser blocks CLI until answer | ☐ |

Paste failures using the template in [`SHORTCOMINGS_IDENTIFICATION_GUIDE.md`](./SHORTCOMINGS_IDENTIFICATION_GUIDE.md).
