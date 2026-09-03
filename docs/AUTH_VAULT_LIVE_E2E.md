# Auth Vault — Live E2E (must pass before Ready)

> Step-by-step sign-off for **MCP `save_auth` + cross-process `load_auth`**.
> Run this after automated vault gates in [`AUTH_VAULT_MANUAL_TEST.md`](./AUTH_VAULT_MANUAL_TEST.md) §1 pass.
>
> Report failures with [`SHORTCOMINGS_IDENTIFICATION_GUIDE.md`](./SHORTCOMINGS_IDENTIFICATION_GUIDE.md).

---

## 0. Prerequisites

| Item | Check |
|---|---|
| Built CLI **1.0.0** on PATH | `genatp version` |
| Chrome installed | Windows: `Test-Path "C:\Program Files\Google\Chrome\Application\chrome.exe"` |
| OS keychain unlocked | Credential Manager / Keychain / Secret Service |
| MCP client | Cursor or Claude Desktop wired per [`MCP.md`](./MCP.md) |
| Isolated profile (recommended) | `$env:GENATP_USER_DATA_DIR = "$env:TEMP\genatp-vault-e2e\profile"` |

Optional vault override (keeps test data separate):

```powershell
$env:GENATP_VAULT_PATH = "$env:TEMP\genatp-vault-e2e\vault.sqlite"
```

For agent-assisted login steps, set NIM (verified model):

```powershell
$env:NVIDIA_API_KEY = "nvapi-…"
$env:GENATP_LLM_PROVIDER = "nim"
$env:GENATP_LLM_MODEL = "openai/gpt-oss-20b"
$env:RUST_LOG = "warn"
```

---

## 1. MCP config (headed + vault scope)

Cursor (`.cursor/mcp.json` or Settings → MCP):

```json
{
  "mcpServers": {
    "genatp": {
      "command": "genatp",
      "args": ["serve", "--stdio", "--headed", "--vault-domains", "github.com,example.com"],
      "env": {
        "RUST_LOG": "warn",
        "GENATP_USER_DATA_DIR": "C:\\Users\\YOU\\AppData\\Local\\Temp\\genatp-vault-e2e\\profile"
      }
    }
  }
}
```

Use absolute paths on Windows if `genatp` is not on PATH. Restart the MCP client after editing.

**Expected:** MCP connects; tools include `navigate`, `snapshot`, `save_auth`, `load_auth`, `list_auth`.

---

## 2. Live E2E steps

Use a **test login site you control**, or GitHub (real MFA). Fill **Actual** as you go.

| Step | Action | Expected | Actual |
|---:|---|---|---|
| 1 | `genatp vault path` (CLI) | Absolute path to `vault.sqlite` | |
| 2 | `genatp vault list` | Empty or no target domain yet | |
| 3 | MCP `navigate` → `https://github.com/login` (or test site) | Login page loads | |
| 4 | MCP `load_auth` `{ "domain": "github.com" }` | First run: no entries / not hydrated | |
| 5 | **You** complete login in headed browser (password + MFA) | Logged-in UI visible | |
| 6 | MCP `save_auth` `{ "domain": "github.com" }` | `saved N auth entries for github.com`, N > 0 | |
| 7 | MCP `list_auth` | Shows `github.com` with N > 0 | |
| 8 | CLI `genatp vault list` | Same domain with entry count | |
| 9 | **Kill** MCP server completely (stop Cursor MCP or kill `genatp serve`) | Process dead | |
| 10 | Restart MCP with **same** `GENATP_VAULT_PATH` / default vault | Server up | |
| 11 | MCP `navigate` → `https://github.com`, then `load_auth` github.com | Session restored; no login wall | |
| 12 | Open a logged-in-only page (notifications / settings) | No re-auth prompt | |
| 13 | CLI `genatp vault revoke github.com` | Domain removed | |
| 14 | MCP restart → `navigate` → `load_auth` github.com | Login wall returns | |

**Pass:** steps 6–12 succeed; step 14 confirms revoke works.

---

## 3. Profile isolation vs CLI

- MCP and CLI each launch Chrome. Without `--user-data-dir`, genATP **1.0.0+** uses a **unique temp profile per process** — vault cookies still hydrate via `load_auth`.
- For **persistent** logins in the browser UI (not just vault), pass the same `--user-data-dir` to MCP and CLI.
- If Chrome exits with **status 21**, use an isolated `--user-data-dir` (see [`USER_MANUAL.md`](./USER_MANUAL.md) § Troubleshooting).

---

## 4. Negative cases (quick)

| Case | Steps | Expected |
|---|---|---|
| Wrong domain on load | Save `github.com`, `load_auth` for `example.com` | No github cookies on example.com |
| Vault scope deny | `serve --vault-domains example.com`, `save_auth` github.com | Capability denied / clear error |
| Stolen DB only | Copy `vault.sqlite` to another user without keychain | Decrypt fails (see [`VAULT.md`](./VAULT.md)) |

---

## 5. Shortcoming template pointer

On failure, copy the template from [`SHORTCOMINGS_IDENTIFICATION_GUIDE.md`](./SHORTCOMINGS_IDENTIFICATION_GUIDE.md) and include:

- MCP client + `genatp version`
- Vault path (`genatp vault path`)
- Step number from §2
- stderr / MCP tool response
