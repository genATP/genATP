# Auth Vault — Complete Manual Test Guide

> Goal: prove **log in once → stay logged in across process restarts** on
> MCP, CLI, and Python SDK. Use NVIDIA NIM for any agent loop steps.
>
> Companion design doc: [`VAULT.md`](./VAULT.md) · automated gates:
> [`TESTING.md`](./TESTING.md) · **live sign-off:** [`AUTH_VAULT_LIVE_E2E.md`](./AUTH_VAULT_LIVE_E2E.md)

All commands assume the **repo root** (`E:\genATP\genATP` or your clone).
PowerShell and Bash variants are shown where they differ.

---

## 0. Prerequisites

| Requirement | Check |
|---|---|
| Chrome / Chromium installed | Windows: `Test-Path "C:\Program Files\Google\Chrome\Application\chrome.exe"` |
| OS keychain unlocked | Windows Credential Manager / macOS Keychain / Linux Secret Service |
| NVIDIA NIM key (for agent steps) | `echo $env:NVIDIA_API_KEY` (PowerShell) or `echo $NVIDIA_API_KEY` |
| Built CLI | `cargo build --release -p genatp-cli` then add `target\release` to `PATH` |

```powershell
# PowerShell — env for this session
$env:NVIDIA_API_KEY = "nvapi-…"
$env:GENATP_LLM_PROVIDER = "nim"
$env:GENATP_LLM_MODEL = "openai/gpt-oss-20b"
$env:RUST_LOG = "warn"
```

```bash
# Bash
export NVIDIA_API_KEY=nvapi-…
export GENATP_LLM_PROVIDER=nim
export GENATP_LLM_MODEL=openai/gpt-oss-20b
export RUST_LOG=warn
```

Optional vault path override (keeps test data out of the default location):

```powershell
$env:GENATP_VAULT_PATH = "$env:TEMP\genatp-vault-test\vault.sqlite"
```

---

## 1. Automated vault gates (run first)

Do these before any manual browser work. They do **not** need NIM.

```bash
cargo test -p genatp-vault --lib
cargo test -p genatp-vault --test property
cargo test -p genatp-vault --test security
# Optional (real OS keychain — may prompt):
cargo test -p genatp-vault --test os_keychain -- --ignored --nocapture
```

| Suite | Expected |
|---|---|
| `--lib` | All pass (crypto, schema, capability) |
| `property` | All pass (wrong-domain DEK, AAD, determinism) |
| `security` | All pass (no plaintext on disk, tamper reject) |
| `os_keychain --ignored` | Pass on a machine with unlocked keychain |

**Pass/fail:** if any non-ignored suite fails, **stop** — do not continue manual tests.

---

## 2. CLI vault surface

### 2.1 Path

```bash
genatp vault path
```

**Expected:** prints one absolute path. On Windows default (no override):

`%APPDATA%\genatp\genatp\data\vault.sqlite`

Resolution order (first hit wins) — see [`VAULT.md`](./VAULT.md):

1. `$GENATP_VAULT_PATH`
2. `$GENATP_HOME/vault.sqlite`
3. Platform project dirs

### 2.2 List (empty / populated)

```bash
genatp vault list
```

**Expected (empty):** either “no domains” / empty table, or `NotInitialized` messaging if vault never opened.

**Expected (after a save):** one line per domain with entry count and updated timestamp, e.g.:

```text
github.com                                  4   2026-…
```

### 2.3 Revoke

```bash
genatp vault revoke github.com
genatp vault list
```

**Expected:** `github.com` gone (or count zero). Next agent/MCP session for that domain must re-authenticate.

---

## 3. MCP path (full login cycle)

Wire MCP (Cursor or Claude Desktop) per [`MCP.md`](./MCP.md):

```json
{
  "mcpServers": {
    "genatp": {
      "command": "genatp",
      "args": ["serve", "--stdio"],
      "env": {
        "RUST_LOG": "warn",
        "NVIDIA_API_KEY": "nvapi-…",
        "GENATP_LLM_PROVIDER": "nim"
      }
    }
  }
}
```

Use **headed** Chrome if your MCP session supports it (or run with
`GENATP_USER_DATA_DIR` unset so each process is clean except vault).

### Steps

| Step | Action | Expected |
|---:|---|---|
| 1 | `navigate` → `https://github.com/login` | Page loads |
| 2 | `load_auth` `{ "domain": "github.com" }` | First time: no/zero entries hydrated |
| 3 | **You** log in interactively (password + MFA) in the browser | Logged-in UI |
| 4 | `save_auth` `{ "domain": "github.com" }` | Message like `saved N auth entries for github.com` |
| 5 | `list_auth` | Shows `github.com` with N > 0 |
| 6 | **Kill** MCP / `genatp serve` completely | Process dead |
| 7 | Restart MCP, `navigate` → github.com, `load_auth` github.com | Hydrates N entries; **no** login form (or session restored) |
| 8 | Confirm you can open notifications / settings without re-login | Pass |

### Negative MCP cases

| Case | Steps | Expected |
|---|---|---|
| Revoke then load | `genatp vault revoke github.com` → MCP `load_auth` | No hydration; login wall returns |
| Wrong domain | Save for `github.com`, `load_auth` for `evil.com` | No github cookies applied to evil.com |
| Stolen file only | Copy `vault.sqlite` to another OS user without keychain KEK | Open fails / cannot decrypt (see threat model in VAULT.md) |

---

## 4. Python SDK path

**As of Wave-1 docs:** high-level `Agent.run` launches a fresh browser each
call. Vault persistence is via MCP tools or native vault APIs — confirm
whether your SDK build exposes `save_auth`/`load_auth` on the agent or only
via MCP. Record what you observe in the shortcomings report.

Minimal smoke (NIM):

```python
from genatp import Agent

agent = Agent(
    provider="nim",
    model="openai/gpt-oss-20b",
    headless=False,
    max_tokens=80_000,   # never leave unlimited on NIM
    max_steps=25,
)

r1 = agent.run(
    "Open https://github.com . If logged out, stop and say LOGIN_REQUIRED. "
    "If logged in, list the first 3 notification titles.",
    start_url="https://github.com",
)
print(r1.outcome, r1.spend.tokens, r1.summary[:500])
```

**Manual vault bridge for SDK testing today:**

1. Complete MCP save_auth for `github.com` (section 3).
2. Run the Python script twice **without** logging in again.
3. **Pass** if both runs see an authenticated session (cookies from vault
   must be wired into the SDK browser launch — if not, mark as shortcoming:
   `SDK does not hydrate Auth Vault on Agent.run`).

---

## 5. Pass / fail checklist

| # | Check | Pass? |
|---|---|---|
| A | Vault unit + property + security tests green | ☐ |
| B | `genatp vault path` prints a real path | ☐ |
| C | `save_auth` after interactive login persists domain | ☐ |
| D | Kill process → `load_auth` → still logged in | ☐ |
| E | `vault revoke` forces re-login | ☐ |
| F | Tamper / wrong-domain behaviour matches VAULT.md | ☐ |
| G | Python/NIM run does not require typing password again (if vault wired) | ☐ |
| H | No plaintext password/cookie strings in `vault.sqlite` (spot-check or rely on security tests) | ☐ |

**Sign-off:** all of A–F must be ☑. G–H required for “seamless vault” user claim.

---

## 6. Shortcoming report (paste back)

If anything fails, copy:

```text
SURFACE: MCP | CLI | Python | Node
SCENARIO: vault save/load / revoke / …
REPRO: (exact commands)
EXPECTED:
ACTUAL:
TOKENS / ERROR:
OS / Chrome version:
```
