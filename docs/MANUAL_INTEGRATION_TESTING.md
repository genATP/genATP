# genATP — Manual Integration Testing Guide

> **Audience:** QA engineers, CI/CD maintainers, and developers performing pre-release validation.
> **Scope:** All phases implemented through v0.3.0 (MCP server, Auth Vault, Cassette recording + replay).
> **Last updated:** 2026-06-05

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [Build & Install](#2-build--install)
3. [CLI Smoke Tests](#3-cli-smoke-tests)
4. [MCP Server — stdio Transport](#4-mcp-server--stdio-transport)
5. [MCP Server — HTTP Transport](#5-mcp-server--http-transport)
6. [Auth Vault — Full Lifecycle](#6-auth-vault--full-lifecycle)
7. [Cassette Recording Pipeline](#7-cassette-recording-pipeline)
8. [Cassette Replay — Deterministic Zero-LLM](#8-cassette-replay--deterministic-zero-llm)
9. [Real-World End-to-End Scenario: MCP + Vault + Cassettes](#9-real-world-end-to-end-scenario-mcp--vault--cassettes)
10. [Script-Based Automation Tests](#10-script-based-automation-tests)
11. [Troubleshooting & Diagnostics](#11-troubleshooting--diagnostics)
12. [Test Matrix & Pass Criteria](#12-test-matrix--pass-criteria)

---

## 1. Prerequisites

### 1.1 System Requirements

| Component | Required | Notes |
|-----------|----------|-------|
| Rust toolchain 1.78+ | Yes | `rustup show` to verify; `rustup update` if behind |
| Chrome / Chromium | Yes | Any recent stable build; auto-detected on macOS |
| OS keychain | For vault tests | macOS Keychain, Windows Credential Manager, Linux Secret Service |
| `cargo-deny` | Optional | `cargo install cargo-deny` |
| `cargo-audit` | Optional | `cargo install cargo-audit` |

### 1.2 Environment Variables

Copy the template and configure ONE provider:

```bash
cp .env.example .env
```

The factory auto-selects a provider based on which API keys are set:

| Priority | Key Present | Provider Used |
|----------|-------------|---------------|
| 1 | `ANTHROPIC_API_KEY` | Anthropic Claude |
| 2 | `NVIDIA_API_KEY` | NVIDIA NIM |
| 3 | `OPENAI_API_KEY` | OpenAI |
| 4 | (none set) | Ollama (local, requires `ollama serve`) |

For cassette recording tests you need a live LLM provider. For replay and
vault tests you don't — they run locally with zero LLM calls.

```bash
# Bash (macOS / Linux / WSL)
set -a; source .env; set +a

# PowerShell (Windows)
#   Use the provided script:
#   .\scripts\test-auth-vault-http.ps1 -LoadEnv
#   Or load manually:
#   Get-Content .env | ForEach-Object {
#     if ($_ -match '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$') {
#       Set-Item -Path "env:$($matches[1])" -Value $matches[2].Trim().Trim('"')
#     }
#   }

# Or use direnv for auto-load on cd (any shell)
echo dotenv > .envrc && direnv allow
```

### 1.3 Quick Verification

```bash
# Verify Rust is ready
cargo --version
# rustc 1.78.0 or later

# Verify Chrome is findable
# macOS (auto-detected):
ls /Applications/Google\ Chrome.app/Contents/MacOS/Google\ Chrome
# Windows (manually set):
$env:CHROME_PATH = "C:\Program Files\Google\Chrome\Application\chrome.exe"
# Linux:
which google-chrome || which chromium-browser || which chromium
```

---

## 2. Build & Install

### 2.1 Debug Build (fast iteration)

```bash
cargo build -p genatp-cli
```

### 2.2 Release Build (for performance testing)

```bash
cargo build --release -p genatp-cli
```

### 2.3 Install to PATH (optional)

```bash
cargo install --path crates/genatp-cli
# Binary at ~/.cargo/bin/genatp
```

### 2.4 Verify the Binary

```bash
# Expected output:
cargo run --release -p genatp-cli -- version
# genatp 0.3.0

# Or if installed:
genatp version
# genatp 0.3.0
```

---

## 3. CLI Smoke Tests

Run these first to confirm the binary parses commands and exits cleanly.
These tests do NOT require a network connection, Chrome, or any API key.

### 3.1 No Arguments

```bash
cargo run --release -p genatp-cli
```

**Expected output:**
```
genatp 0.3.0 — try `genatp --help`
```

**Exit code:** 0

---

### 3.2 Help

```bash
cargo run --release -p genatp-cli -- --help
```

**Expected output:** Shows all subcommands:
```
genATP — agent-grade browser automation

Usage: genatp [OPTIONS] [COMMAND]

Commands:
  version   Print version and exit
  serve     Start an MCP server (stdio or HTTP transport)
  vault     Manage the encrypted auth vault (cookies + localStorage)
  record    Record a successful agent run as a signed cassette
  cassette  Manage cassettes (list, inspect, replay)
  help      Print this message or the help of the given subcommand

Options:
  -v, --verbose...  Verbose logging (-v = INFO, -vv = DEBUG)
  -h, --help        Print help
  -V, --version     Print version
```

---

### 3.3 Version

```bash
cargo run --release -p genatp-cli -- version
```

**Expected output:** `genatp 0.3.0`

**Exit code:** 0

---

### 3.4 Vault Path

```bash
cargo run --release -p genatp-cli -- vault path
```

**Expected output:** A file system path. The exact path depends on your OS:

| OS      | Typical Vault Path |
|---------|-------------------|
| Windows | `%APPDATA%\genatp\genatp\data\vault.sqlite` → `C:\Users\<you>\AppData\Roaming\genatp\genatp\data\vault.sqlite` |
| macOS   | `~/Library/Application Support/genatp/genatp/data/vault.sqlite` |
| Linux   | `~/.local/share/genatp/genatp/data/vault.sqlite` |

> ⚠ **Important:** The storage paths throughout this guide are platform-dependent.
> All paths shown with `~/.genatp/` below are **examples** — the actual path is
> determined by `directories::ProjectDirs::from("dev", "genatp", "genatp")`.
> Use `genatp vault path` to find the real vault location on your machine, and
> `genatp cassette list` to find cassette locations.

**Exit code:** 0

---

### 3.5 Vault List (empty vault)

```bash
cargo run --release -p genatp-cli -- vault list
```

**Expected output:**
```
vault not initialised yet — log in via an agent first
```

**Exit code:** 0

---

### 3.6 Cassette List (no cassettes)

```bash
cargo run --release -p genatp-cli -- cassette list
```

**Expected output:**
```
no cassettes recorded yet
```

**Exit code:** 0

---

### 3.7 Cassette Inspect (non-existent slug)

```bash
cargo run --release -p genatp-cli -- cassette inspect non-existent-slug
```

**Expected output:** An error message indicating the slug was not found.

```
genatp: cassette failed: no cassettes for slug `non-existent-slug`
```

**Exit code:** 1

---

### 3.8 Cassette Replay (non-existent slug)

```bash
cargo run --release -p genatp-cli -- cassette replay non-existent-slug
```

**Expected output:**
```
genatp: cassette failed: no cassettes for slug `non-existent-slug`
```

**Exit code:** 1

---

### 3.9 Serve Help

```bash
cargo run --release -p genatp-cli -- serve --help
```

**Expected output:** Shows serve-specific flags:
```
Start an MCP server (stdio or HTTP transport)

Usage: genatp serve [OPTIONS]

Options:
      --stdio                       Use the stdio transport (default)
      --port <PORT>                 Bind HTTP transport on 127.0.0.1:<port>
      --headed                      Launch Chrome in headed mode
      --user-data-dir <USER_DATA_DIR>
  -h, --help                        Print help
```

---

### 3.10 Record Help

```bash
cargo run --release -p genatp-cli -- record --help
```

**Expected output:** Shows record arguments:
```
Record a successful agent run as a signed cassette

Usage: genatp record [OPTIONS] <SLUG> <URL> <GOAL>

Arguments:
  <SLUG>  Human-readable slug for the cassette
  <URL>   URL to start the agent on
  <GOAL>  Free-form goal for the agent

Options:
      --model <MODEL>          LLM model to use [default: gpt-4]
      --max-steps <MAX_STEPS>  Maximum agent steps before giving up [default: 30]
      --headed                 Launch Chrome in headed mode
      --description <DESCRIPTION>
  -h, --help                   Print help
```

---

## 4. MCP Server — stdio Transport (for MCP clients only)

The stdio transport speaks JSON-RPC 2.0 over **stdin/stdout** (standard
input and output). It is the primary way Claude Desktop, Cursor, and the
MCP Inspector connect to genATP.

> ⚠ **You cannot test stdio with curl or PowerShell `Invoke-RestMethod`.**
> The stdio transport does NOT bind an HTTP port. It reads JSON-RPC from
> the process's **standard input** — which means only Claude Desktop,
> Cursor, or the `mcp_client.ps1` script can communicate with it.
>
> **For manual testing with curl / PowerShell, use the HTTP transport:**
> ```bash
> cargo run --release -p genatp-cli -- serve --port 3000 --headed
> ```
> See §5 for the full HTTP testing guide.

### 4.1 Basic stdio Initialization

Start the server:

```bash
cargo run --release -p genatp-cli -- serve --stdio --headed -vv
```

> 💡 **Chrome is lazy-launched.** The server process starts immediately
> and waits for JSON-RPC on stdin. Chrome will NOT appear yet — it only
> launches on the first browser command (`navigate`, `snapshot`, etc.).
> See §4.2 for how to trigger it.

**Terminal B** — use the PowerShell test script to send initialize +
tools/list:

```powershell
.\mcp_client.ps1
```

**Expected output:**
```
INIT RESPONSE: {"jsonrpc":"2.0","id":1,..."serverInfo":{"name":"genatp","version":"0.3.0"}...}
TOOLS RESPONSE: {"jsonrpc":"2.0","id":2,..."tools":[{"name":"navigate",...},{"name":"snapshot",...},{"name":"click",...},...]}
```

Verify the following tools are present:
- `navigate`
- `snapshot`
- `click`
- `fill`
- `extract`
- `scroll`
- `screenshot`
- `get_url`
- `go_back`
- `save_auth` (if Phase 6 vault is wired)
- `load_auth` (if Phase 6 vault is wired)
- `list_auth` (if Phase 6 vault is wired)

---

### 4.2 Testing stdio with the Script

The only reliable way to test the stdio transport manually is via the
`mcp_client.ps1` PowerShell script, which correctly pipes JSON-RPC
messages through stdin/stdout:

```powershell
.\mcp_client.ps1
```

This script:
1. Spawns its own `genatp.exe serve --stdio` subprocess
2. Sends `initialize` with id=1
3. Sends `tools/list` with id=2
4. Prints both responses
5. Kills the subprocess

**For all interactive browser testing (navigate, click, fill, snapshot, etc.),**
**use the HTTP transport instead:**

```bash
cargo run --release -p genatp-cli -- serve --port 3000 --headed
```

Continue with §5 for the full HTTP testing guide.

---

## 5. MCP Server — HTTP Transport

The HTTP transport speaks JSON-RPC 2.0 over HTTP on `127.0.0.1:<port>`.
This is the best transport for **manual testing** with `curl` or
PowerShell `Invoke-RestMethod`.

### 5.1 Start HTTP Server

Start the server with `--port` (NOT `--stdio`):

```bash
cargo run --release -p genatp-cli -- serve --port 3000 --headed -vv
```

> 💡 **Chrome is lazy-launched** — it will NOT appear yet. Chrome only
> launches on the first browser command (`navigate`, `snapshot`, etc.).
> Sending `initialize` and `tools/list` does not launch it.

Expected output on stderr:
```
2026-06-05T10:00:00.000000Z  INFO genatp_mcp::http: starting MCP HTTP server addr=127.0.0.1:3000
```

### 5.2 Health Check

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:3000/healthz
```

**Expected:** `200 OK` (empty body or `ok`)

### 5.3 Full Tool Round-Trip

Send a complete initialize + tool call sequence. Following the MCP
protocol, you MUST send `notifications/initialized` after `initialize`.

**Step 1 — Initialize:**

```powershell
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"powershell-test","version":"1"}}}' |
  ConvertTo-Json -Depth 10
```

**Response:** Server info + capabilities.

**Step 2 — Send initialized notification (REQUIRED):**

```powershell
# Notifications have no response — pipe to Out-Null to suppress empty output
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","method":"notifications/initialized"}' | Out-Null
```

**Step 3 — List tools:**

```powershell
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' |
  ConvertTo-Json -Depth 10
```

**Expected output:** Full JSON with 12 tool descriptors (names like `navigate`, `snapshot`, `click`, etc.)

**Step 4 — Navigate (🔥 Chrome launches here):**

```powershell
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://example.com"}}}' |
  ConvertTo-Json -Depth 10
```

> 💡 **Chrome appears NOW** — a visible browser window opens and
> navigates to `https://example.com`. This is the first tool call that
> needs a browser page, so the engine launches Chrome lazily.

**Expected output:** `"isError": false` with `"text": "navigated to https://example.com/"`

**Step 5 — Snapshot:**

```powershell
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"snapshot"}}' |
  ConvertTo-Json -Depth 10
```

**Expected output:** Compressed accessibility tree JSON with `@e1`, `@e2`, etc. refs.

**Step 6 — Get current URL:**

```powershell
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"get_url"}}' |
  ConvertTo-Json -Depth 10
```

**Expected output:** `"text": "https://example.com/"`

### 5.4 Real Site: GitHub.com Navigation

With the server still running (`--port 3000 --headed`):

```powershell
# Navigate to GitHub
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://github.com"}}}' |
  ConvertTo-Json -Depth 10
```

```powershell
# Snapshot
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"snapshot"}}' |
  ConvertTo-Json -Depth 10
```

**Expected output:** A JSON tree with interactive elements like:
- `@e1` — link to "Sign in"
- `@e2` — link to "Sign up"
- Search field, navigation links, etc.

```powershell
# Extract visible text
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"extract","arguments":{"query":""}}}'
```

**Expected output:** Visible text content from the GitHub homepage
including "Let's build from here" or similar.

```powershell
# Scroll down
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"scroll","arguments":{"dx":0,"dy":500}}}'
```

**Expected output:** `"isError":false`

```powershell
# Take a screenshot
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"screenshot"}}'
```

**Expected output:** A base64-encoded PNG image.

```powershell
# Go back
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"go_back"}}'
```

**Expected output:** `"text": "no prior page"` or `"navigated back"`.

### 5.5 Real Site: Fill + Click on a Form

**Test site:** `https://www.automationtesting.co.uk/contactForm.html` (a contact form with First Name, Last Name, Email, and Comments fields)

> ⚠ **httpbin.org is no longer available (returns 503).**
> `automationtesting.co.uk` is used instead. The form fields are:
> First Name (textbox), Last Name (textbox), Email Address (textbox),
> Comments (textarea), RESET (button), SUBMIT (button).

```powershell
# Navigate to the form
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://www.automationtesting.co.uk/contactForm.html"}}}' |
  ConvertTo-Json -Depth 10
```

```powershell
# Snapshot — find the @eN refs for form elements
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"snapshot"}}' |
  ConvertTo-Json -Depth 10
```

Note the `@eN` refs for the form fields. Expected refs from the snapshot:
- `@e442` (or similar) — First Name
- `@e443` (or similar) — Last Name
- `@e444` (or similar) — Email Address
- `@e445` (or similar) — Comments
- `@e448` (or similar) — SUBMIT button

```powershell
# Fill the "First Name" field (replace @eX with actual ref from snapshot)
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"fill","arguments":{"ref":"@e442","value":"John"}}}'
```

```powershell
# Fill the "Last Name" field
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"fill","arguments":{"ref":"@e443","value":"Doe"}}}'
```

```powershell
# Fill the "Email Address" field
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"fill","arguments":{"ref":"@e444","value":"john@example.com"}}}'
```

```powershell
# Fill the "Comments" field
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"fill","arguments":{"ref":"@e445","value":"This is a test message to verify the form submission functionality."}}}'
```

```powershell
# Click the SUBMIT button (replace @eX with actual ref)
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"click","arguments":{"ref":"@e448"}}}'
```

**Expected output:** The form submits and the page navigates to
`https://www.automationtesting.co.uk/contact-form-thank-you.html`
showing: **"Thank you for your mail!"**

### 5.6 Verbose Logging

Test with debug logging to troubleshoot:

```powershell
# Terminal A: Start with RUST_LOG=debug
$env:RUST_LOG = "debug"
cargo run --release -p genatp-cli -- serve --port 3000 --headed -vv
```

In another terminal, send a navigate command:

```powershell
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://example.com"}}}' |
  ConvertTo-Json -Depth 10
```

The stderr output shows:
- `DEBUG` — every CDP command sent to Chrome
- `INFO` — navigation lifecycle events
- `WARN` — any retries or fallbacks

### 5.7 Persistent User Data Directory

Test that cookies persist across restarts (requires `--port`):

```powershell
# Terminal A — first session (use a Windows temp path instead of /tmp)
cargo run --release -p genatp-cli -- serve --port 3000 --user-data-dir "$env:TEMP\genatp-test-profile" --headed
```

```powershell
# Terminal B — navigate to a site that sets cookies
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://httpbin.org/cookies/set?test_cookie=hello123"}}}' |
  ConvertTo-Json -Depth 10
```

Stop the server (Ctrl-C), restart with the same `--user-data-dir`, then:

```powershell
# Check cookies
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://httpbin.org/cookies"}}}' |
  ConvertTo-Json -Depth 10
```

The cookies should still be present. (The vault is the *encrypted*
replacement for this — see §6.)

### 5.8 Error Handling Test

Test that invalid tools return proper errors:

```powershell
# Unknown tool
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nonexistent_tool","arguments":{}}}' |
  ConvertTo-Json -Depth 10
```

**Expected:** `"isError": true` with an appropriate error message.

```powershell
# Navigate to invalid URL
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"not-a-url"}}}' |
  ConvertTo-Json -Depth 10
```

**Expected:** `"isError": true` with URL parse error message.

---

## 6. Auth Vault — Full Lifecycle

### 6.1 Key Generation (Automatic)

The vault initialises on first use. No manual key generation needed.

```bash
cargo run --release -p genatp-cli -- vault path
```

This creates the parent directory if missing but does NOT create the
vault file — that happens on first `save_auth` or `Vault::open`.

### 6.2 Resetting the Vault (for testing)

If the vault gets into a bad state during testing, reset it by deleting the
vault file AND its OS keychain entry:

```bash
# 1. Find the vault path
cargo run --release -p genatp-cli -- vault path

# 2. Delete the SQLite file (path from step 1)
#    On Windows:
#    Remove-Item "$env:APPDATA\genatp\genatp\data\vault.sqlite"
#    On macOS / Linux:
#    rm ~/Library/Application\ Support/genatp/genatp/data/vault.sqlite
#    rm ~/.local/share/genatp/genatp/data/vault.sqlite

# 3. The OS keychain entry is automatically created on next use.
#    If you also need to clear it manually:
#    - macOS: Open Keychain Access → search for "genatp" → delete
#    - Windows: Control Panel → Credential Manager → Windows Credentials → find "genatp"
#    - Linux: `secret-tool search genatp true` and delete with `secret-tool clear`
```

### 6.3 Manual Vault Operations

**List (empty):**

```bash
cargo run --release -p genatp-cli -- vault list
# vault not initialised yet — log in via an agent first
```

**Revoke non-existent domain:**

```bash
cargo run --release -p genatp-cli -- vault revoke nonexistent.com
# no entries for `nonexistent.com` (already revoked or never stored)
```

### 6.3 Full Vault Lifecycle via MCP HTTP

This test exercises the complete vault flow using the real MCP server
against a real website with a login form.

**Step 1:** Start the server:

```bash
cargo run --release -p genatp-cli -- serve --port 3000 --headed
```

Wait for the server to start and Chrome to open.

**Step 2:** Navigate to a test page:

```powershell
# Navigate to a site that may set cookies (for save_auth testing)
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://www.automationtesting.co.uk/contactForm.html"}}}' |
  ConvertTo-Json -Depth 10
```

**Step 3:** Save auth for the domain:

```powershell
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"save_auth","arguments":{"domain":"automationtesting.co.uk"}}}'
```

**Expected output:** A message like `"saved N auth entries for automationtesting.co.uk"`
or `"saved 0 auth entries"` (if no cookies/storage were set).

**Step 4:** Verify via CLI:

```bash
cargo run --release -p genatp-cli -- vault list
```

**Expected output:** A table with the domain, entry count, and timestamp:
```
domain                                      count  updated
automationtesting.co.uk                         0  2026-06-05 10:00:00 UTC
```

**Step 5:** Revoke:

```bash
cargo run --release -p genatp-cli -- vault revoke automationtesting.co.uk
# revoked all entries for `automationtesting.co.uk`
```

**Step 6:** Verify revocation:

```bash
cargo run --release -p genatp-cli -- vault list
# vault is empty — or shows other domains but not automationtesting.co.uk
```

### 6.4 Persistence Across Restarts (The Killer Test)

This is the most important vault test — it proves auth outlives the process.

**Step 1:** Start the server:

```bash
cargo run --release -p genatp-cli -- serve --port 3000 --headed
```

**Step 2:** Save auth for a domain (use a site that sets cookies, like GitHub):

```powershell
# Navigate
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://github.com"}}}' |
  ConvertTo-Json -Depth 10

# Save auth
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"save_auth","arguments":{"domain":"github.com"}}}'
```

**Step 3:** Kill the server (Ctrl-C).

**Step 4:** Start the server again:

```bash
cargo run --release -p genatp-cli -- serve --port 3000 --headed
```

**Step 5:** Load auth and verify:

```powershell
# Navigate (required to set the domain context)
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://github.com"}}}' |
  ConvertTo-Json -Depth 10

# Load auth
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"load_auth","arguments":{"domain":"github.com"}}}'
```

**Expected output:** `"hydrated N auth entries"` — proving the session
survived the process restart.

---

## 7. Cassette Recording Pipeline

### 7.1 Automated Test (No Chrome Needed)

The fastest way to test the recording pipeline:

```bash
cargo test -p genatp-cassette --test e2e_phase7 -- --nocapture
```

**Expected output:** All 14 tests pass with detailed per-test output:
```
test e2e_key_generation ... ok
test e2e_full_recording_pipeline ... ok
test e2e_tamper_detection ... ok
test e2e_list_slugs_and_versions ... ok
test e2e_budget_spend_tracking ... ok
test e2e_hash_chain_integrity ... ok
test e2e_serialization_deserialization_roundtrip ... ok
test e2e_domains_extraction ... ok
test e2e_cassette_file_format_integrity ... ok
test e2e_multiple_domains_extraction ... ok
test e2e_wrong_key_rejected ... ok
test e2e_goal_start_url_metadata ... ok
test e2e_empty_log_rejected ... ok
test e2e_failed_run_rejected ... ok
```

### 7.2 Generate a Real Cassette File (Dry-Run)

> ⚠ **Note:** The `genatp record` CLI command exists but is a **stub** — it prints a message
> and does **not** actually record. Full recording via the CLI requires the orchestrator
> to be fully wired up with a live browser session (planned Phase 7 E2E integration).
> The cassette recording *library* (`CassetteRecorder`) is production-ready and tested
> (see §7.1 automated tests).

The bootstrap example creates a real cassette on disk using the recording library:

```bash
cargo run --release --example bootstrap_cassette -p genatp-cassette
```

This creates a cassette. Find the actual path on your system:

```bash
# The path uses ProjectDirs - find it by inspecting the cassette:
cargo run --release -p genatp-cli -- cassette list
```

**Platform-specific cassette storage locations (for manual inspection):**

| OS      | Typical Cassette Root |
|---------|----------------------|
| Windows | `%APPDATA%\genatp\genatp\data\cassettes\` → `C:\Users\<you>\AppData\Roaming\genatp\genatp\data\cassettes\` |
| macOS   | `~/Library/Application Support/genatp/genatp/data/cassettes/` |
| Linux   | `~/.local/share/genatp/genatp/data/cassettes/` |

```bash
# Example: Inspect the directory structure
# PowerShell:
#   Get-ChildItem "$env:APPDATA\genatp\genatp\data\cassettes" -Recurse
# macOS / Linux:
#   find ~/Library/Application\ Support/genatp/genatp/data/cassettes/ -type f
#   find ~/.local/share/genatp/genatp/data/cassettes/ -type f
```

### 7.3 List Cassettes

```bash
cargo run --release -p genatp-cli -- cassette list
```

**Expected output:**
```
slug                                        versions
example-verify-title                        1.0.0
```

### 7.4 Inspect a Cassette

```bash
cargo run --release -p genatp-cli -- cassette inspect example-verify-title
```

**Expected output:** Full cassette metadata + step listing:
```
Cassette: example-verify-title v1.0.0
  Schema version: 1
  Goal:           Verify the page title at https://example.com
  Start URL:      https://example.com
  Model:          mock
  Recorded at:    2026-06-05T10:00:00Z
  Steps:          1
  Signer:         <64-char-hex-public-key>
  Domains:        example.com
  Signature:      VALID ✓
  Hash chain:     VALID ✓

Steps:
  [ 0] https://example.com → Navigate { url: "https://example.com" } (ok)
```

### 7.5 Inspect a Specific Version

```bash
cargo run --release -p genatp-cli -- cassette inspect example-verify-title --version 1.0.0
```

**Expected output:** Same as above, explicitly loading version 1.0.0.

### 7.6 Verify Cassette Tamper Detection

This test confirms that modifying a cassette file invalidates its signature.

**Step 1:** Find the cassette file using the platform-specific paths from §7.2:

```bash
# PowerShell / Windows:
$cassette = "$env:APPDATA\genatp\genatp\data\cassettes\example-verify-title\versions\1.0.0.cassette"
# macOS:
# CASSETTE=~/Library/Application\ Support/genatp/genatp/data/cassettes/example-verify-title/versions/1.0.0.cassette
# Linux:
# CASSETTE=~/.local/share/genatp/genatp/data/cassettes/example-verify-title/versions/1.0.0.cassette
```

**Step 2:** Create a backup and tamper with the file:

```powershell
# PowerShell
Copy-Item $cassette "$cassette.backup"
$raw = [System.IO.File]::ReadAllBytes($cassette)
$raw[$raw.Length / 2 - 1] = $raw[$raw.Length / 2 - 1] -bxor 0xFF
[System.IO.File]::WriteAllBytes($cassette, $raw)
```

```bash
# macOS / Linux
cp "$CASSETTE" "$CASSETTE.backup"
python3 -c "
import sys
path = '$CASSETTE'
with open(path, 'rb') as f:
    data = bytearray(f.read())
data[len(data)//2 - 1] ^= 0xFF
with open(path, 'wb') as f:
    f.write(data)
"
```

**Step 3:** Attempt to inspect the tampered cassette:

```bash
cargo run --release -p genatp-cli -- cassette inspect example-verify-title
```

**Expected output:**
```
genatp: cassette failed: load failed: signature invalid
```

**Step 4:** Restore the backup:

```bash
# PowerShell
Copy-Item "$cassette.backup" $cassette -Force
# macOS / Linux
cp "$CASSETTE.backup" "$CASSETTE"
```

---

## 8. Cassette Replay — Deterministic Zero-LLM

### 8.1 Automated Replay Tests

```bash
cargo test -p genatp-cassette --test e2e_phase8_replay -- --nocapture
```

**Expected output:** All 3 tests pass:
```
test e2e_phase8_record_save_load_replay ... ok
test e2e_phase8_replay_detects_wrong_refs ... ok
test e2e_phase8_replay_runtime_measurement ... ok
```

### 8.2 Unit Replay Tests

```bash
cargo test -p genatp-cassette -- replay -- --nocapture
```

**Expected output:** All 6 tests pass:
```
test replay::tests::replay_empty_cassette ... ok
test replay::tests::replay_navigate_only ... ok
test replay::tests::replay_navigate_url_matches_step_url ... ok
test replay::tests::replay_ref_resolver_failure_marks_step_failed ... ok
test replay::tests::replay_full_pipeline_with_ref_resolver ... ok
test replay::tests::replay_zero_llm_calls ... ok
```

### 8.3 CLI Dry-Run Replay

Use the cassette created by the bootstrap example:

```bash
cargo run --release -p genatp-cli -- cassette replay example-verify-title
```

**Expected output:**
```
Cassette: example-verify-title v1.0.0 — replaying 1 steps without LLM calls...
  Goal:       Verify the page title at https://example.com
  Start URL:  https://example.com

  ⚠  Full deterministic replay requires a running browser.
     The `genatp-cassette` replay engine is production-ready:
     - Zero LLM calls: actions execute directly from the cassette
     - Ref resolver: Click/Fill actions need live page refs
     - URL tracking: each step checks URL matches recording

  Use `genatp cassette inspect example-verify-title` to see the steps.

  [ 0] ✓ Navigate { url: "https://example.com" }

  Dry-run: 1 would succeed, 0 would need a ref resolver
```

### 8.4 Replay a Specific Version

```bash
cargo run --release -p genatp-cli -- cassette replay example-verify-title --version 1.0.0
```

**Expected output:** Same as above, but explicitly loading version 1.0.0.

---

## 9. Real-World End-to-End Scenario: MCP + Vault + Cassettes

This is the crown-jewel test that exercises the entire product surface
in a realistic flow. It requires a real LLM provider key.

### Scenario: "Log into a service, do something, save as cassette, replay with zero LLM"

**Phase A — Server Setup**

```bash
# Terminal A: Start the MCP server
cargo run --release -p genatp-cli -- serve --port 3000 --headed -vv
```

**Phase B — Login & Auth Vault**

Use Claude Desktop or the MCP Inspector (or manual curl commands) to:

1. `navigate` to a site with a login form
2. `load_auth` — should return 0 entries (first time)
3. Log in interactively (fill username/password, handle any MFA)
4. `save_auth` with `{ "domain": "<site-domain>" }`
5. `list_auth` — confirm entries were saved

```bash
# Verify via CLI
cargo run --release -p genatp-cli -- vault list
```

**Expected:** The domain appears with entry count > 0.

**Phase C — Kill & Restart**

```bash
# Kill the server (Ctrl-C in Terminal A)

# Restart
cargo run --release -p genatp-cli -- serve --port 3000 --headed
```

**Phase D — Reload Auth Without Re-Login**

1. `navigate` to the same site
2. `load_auth` — should return "hydrated N auth entries"
3. `snapshot` — should show the authenticated homepage (NOT the login page)

**Expected:** The user is authenticated without any prompts, proving the
vault works across process restarts.

**Phase E — Record a Cassette (via SDK)**

```python
# This would be the Python SDK record flow (Phase 7 wiring)
# For now, use the Rust bootstrap example:
cargo run --release --example bootstrap_cassette -p genatp-cassette
```

**Phase F — List & Inspect**

```bash
cargo run --release -p genatp-cli -- cassette list
cargo run --release -p genatp-cli -- cassette inspect my-cassette-slug
```

**Phase G — Replay (Dry-Run)**

```bash
cargo run --release -p genatp-cli -- cassette replay my-cassette-slug
```

**Expected:** The CLI shows which steps would succeed and which need a
ref resolver. Zero LLM tokens are consumed during this analysis.

---

## 10. Script-Based Automation Tests

### 10.1 MCP Client Test (PowerShell)

A standalone PowerShell script tests the stdio MCP transport:

```powershell
.\mcp_client.ps1
```

**Expected output:**
```
INIT RESPONSE: {"jsonrpc":"2.0","id":1,...
TOOLS RESPONSE: {"jsonrpc":"2.0","id":2,...
```

### 10.2 Auth Vault HTTP Test (PowerShell)

A comprehensive script that exercises the vault over HTTP:

```powershell
# Step 1: Start the server in Terminal A
cargo run --release -p genatp-cli -- serve --port 3000 --headed

# Step 2: Run the vault test script
.\scripts\test-auth-vault-http.ps1 -LoadEnv

# To test persistence across restarts:
# After save_auth completes, Ctrl-C the server, restart, then:
.\scripts\test-auth-vault-http.ps1 -LoadOnly -LoadEnv
```

The script:
1. Loads `.env` (optional `-LoadEnv`)
2. Initializes MCP connection
3. Lists available tools
4. Navigates to the login URL
5. Waits for manual login (press Enter when done)
6. Calls `save_auth`
7. If `-LoadOnly` is used, restarts and calls `load_auth`

### 10.3 Full Test Suite (All Automated Tests)

This is the first command to run on every build:

```bash
# All workspace tests (unit + integration + E2E)
cargo test --workspace -- --nocapture 2>&1 | grep -E "^(test |running |test result)"

# Expected: ~235 tests, 0 failed
```

**Per-crate breakdown:**

| Crate | Tests | Expected |
|-------|-------|----------|
| genatp-atp-spec | 1 | All pass |
| genatp-browser | 26 | All pass (may need Chrome) |
| genatp-cassette | 40 | All pass (Phase 7 + Phase 8) |
| genatp-cli | 13 | All pass |
| genatp-core | 27 | All pass |
| genatp-debugger | 1 | All pass |
| genatp-llm | 27 | All pass |
| genatp-mcp | 22 | All pass |
| genatp-node | 1 | All pass |
| genatp-perception | 36 | All pass |
| genatp-registry | 1 | All pass |
| genatp-vault | 40 | All pass (3 ignored: OS keychain) |
| **Total** | **235** | **All pass** |

### 10.4 Quick Pre-Release Gate Script

Create this local script for rapid regression checking:

```bash
#!/usr/bin/env bash
set -euo pipefail

echo "=== Phase 1: Static gates ==="
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check

echo "=== Phase 2: Unit + Integration ==="
cargo test --workspace -- --nocapture

echo "=== Phase 3: Cassette recording E2E ==="
cargo test -p genatp-cassette --test e2e_phase7 -- --nocapture

echo "=== Phase 4: Cassette replay E2E ==="
cargo test -p genatp-cassette --test e2e_phase8_replay -- --nocapture

echo "=== Phase 5: Vault security ==="
cargo test -p genatp-vault --test security

echo "=== ALL GATES PASSED ==="
```

---

## 11. Troubleshooting & Diagnostics

### 11.1 Server Won't Start

**Symptom:** `genatp serve` exits immediately with an error.

**Checklist:**
1. Is Chrome installed? Run `which google-chrome` (Linux/macOS) or
   check `C:\Program Files\Google\Chrome` (Windows).
2. Set `CHROME_PATH` explicitly if auto-detection fails.
3. Run with `RUST_LOG=debug genatp serve --stdio --headed` to see
   detailed logs.
4. Is port 3000 already in use? Try `--port 3001`.

### 11.2 Tools Return `"ref @eN not in current snapshot"`

**Symptom:** Click or Fill commands fail because the agent forgot to
call `snapshot` first.

**Fix:** Always call `snapshot()` after `navigate()` to get fresh `@eN`
refs. The refs are valid until the next navigation or page mutation.

### 11.3 Cassette Load Fails with "signature invalid"

**Symptom:** `genatp cassette inspect` returns a signature error.

**Causes:**
- The cassette file was tampered with (expected! this is a security feature)
- The signing key was regenerated (`~/.genatp/keys/signing.key` was deleted)
- The file is corrupted (disk error)

**Diagnostics:**

First find the actual paths on your system:
```bash
# Find vault path
cargo run --release -p genatp-cli -- vault path
```

Then check the key and cassette directories using the platform paths
from §3.4 and §7.2. For example on macOS:
```bash
ls ~/Library/Application\ Support/genatp/genatp/data/keys/
# Should show signing.key and signing.pub

ls ~/Library/Application\ Support/genatp/genatp/data/cassettes/<slug>/versions/
# Should show .cassette files > 64 bytes

# Regenerate keys if needed (deletes old ones)
# First find the keys directory, then:
#   rm -rf <keys_dir>/
# Then run any cassette command to regenerate
cargo run --release -p genatp-cli -- cassette list
```

### 11.4 Vault Shows "not initialised"

**Symptom:** `genatp vault list` says vault not initialised.

**Fix:** This is normal for a fresh install. The vault is automatically
initialised on first `save_auth` (MCP tool) or `Vault::open()` call.
Simply use an agent to log into a site and call `save_auth`.

### 11.5 "no cassettes for slug" Error

**Symptom:** Inspecting or replaying a cassette returns slug-not-found.

**Causes:**
- The slug doesn't exist (typo?)
- The cassette was saved by a different signing key
- The cassette storage directory was cleared

**Fix:**
```bash
# List all available slugs
cargo run --release -p genatp-cli -- cassette list

# Check the directory directly (use platform path from §7.2)
# macOS:
#   ls ~/Library/Application\ Support/genatp/genatp/data/cassettes/
# Windows (PowerShell):
#   Get-ChildItem "$env:APPDATA\genatp\genatp\data\cassettes"
# Linux:
#   ls ~/.local/share/genatp/genatp/data/cassettes/
```

### 11.6 Cassette Replay Shows Zero Steps

**Symptom:** A cassette exists but `replay` shows 0 steps.

**Fix:** The cassette was recorded with no Snapshot+Action pairs (see
§7.2 for how the recorder pairs events). Ensure the event log contains
at least one `Snapshot` event followed by an `ActionResult` event.

### 11.7 Chrome Doesn't Open in Headed Mode

**Symptom:** The `--headed` flag is set but no Chrome window appears.

**Fix:** On some systems, Chrome's headless mode is configured at the
browser level. Try:
```bash
# macOS
export CHROME_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"

# Windows (PowerShell)
$env:CHROME_PATH = "C:\Program Files\Google\Chrome\Application\chrome.exe"
```

### 11.8 Tests Fail: AccessibilityTree::from_nodes

**Symptom:** Tests fail because `AccessibilityTree::from_nodes` doesn't
exist.

**Fix:** This is a `genatp-browser` crate function. Ensure you're on the
latest build:
```bash
cargo build --release -p genatp-browser -p genatp-cassette
```

### 11.9 Windows-Specific Issues

**PowerShell quoting:** When passing JSON with `Invoke-RestMethod` on Windows,
use single quotes for the JSON body string. PowerShell single-quoted strings
are verbatim — no escaping needed:

```powershell
# Recommended: inline body with Invoke-RestMethod
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'

# Alternative: PowerShell with a here-string (better for long bodies)
$body = @'
{"jsonrpc":"2.0","id":1,"method":"tools/list"}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body $body
```

**Path separators:** Always use forward slashes `/` in URLs. Use
backslashes `\` or forward slashes `/` for file paths (PowerShell does
NOT require escaping backslashes in strings).

**Long paths:** If you get "path too long" errors on Windows, enable
long path support:
```powershell
# Run as Administrator
New-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" -Name "LongPathsEnabled" -Value 1 -PropertyType DWORD -Force
```

---

## 12. Test Matrix & Pass Criteria

### 12.1 Gate Checklist

Before every release, run through this checklist:

| # | Test | Command | Pass Criteria |
|---|------|---------|---------------|
| 1 | Static analysis | `cargo fmt --all -- --check` | No formatting errors |
| 2 | Linting | `cargo clippy --workspace --all-targets -- -D warnings` | Zero warnings |
| 3 | Security audit | `cargo deny check` | All green |
| 4 | Full test suite | `cargo test --workspace` | 235+ passed, 0 failed |
| 5 | Vault unit tests | `cargo test -p genatp-vault --lib` | 36 passed, 0 failed |
| 6 | Vault security | `cargo test -p genatp-vault --test security` | 4 passed, 0 failed |
| 7 | Cassette unit | `cargo test -p genatp-cassette --lib` | 39 passed, 0 failed |
| 8 | Cassette E2E P7 | `cargo test -p genatp-cassette --test e2e_phase7` | 14 passed, 0 failed |
| 9 | Cassette E2E P8 | `cargo test -p genatp-cassette --test e2e_phase8_replay` | 3 passed, 0 failed |
| 10 | CLI tests | `cargo test -p genatp-cli` | 13 passed, 0 failed |
| 11 | MCP tests | `cargo test -p genatp-mcp --tests` | 22 passed, 0 failed |
| 12 | CLI dry-run | `genatp cassette replay example-verify-title` | Prints dry-run analysis |
| 13 | Vault CLI | `genatp vault list` | Clean output, no panic |
| 14 | OS keychain | `cargo test -p genatp-vault --test os_keychain -- --ignored` | 3 passed (per-platform) |
| 15 | HTTP MCP start | `genatp serve --port 3000` | Server starts, healthz returns 200 |
| 16 | MCP roundtrip | Send `tools/list` to HTTP server | Returns 9+ tools |

### 12.2 Acceptance Test: Cassette File Format

Run this to verify the on-disk format. First find the cassette path:

```bash
# Find the actual cassette path
cargo run --release -p genatp-cli -- cassette list
cargo run --release -p genatp-cli -- cassette inspect example-verify-title
```

Then locate the file on disk using the platform-specific paths from §7.2.

On Windows (PowerShell):
```powershell
$cassette = "$env:APPDATA\genatp\genatp\data\cassettes\example-verify-title\versions\1.0.0.cassette"
$raw = [System.IO.File]::ReadAllBytes($cassette)

# Verify: must be > 64 bytes (64-byte signature at end)
$raw.Length -gt 64
# → True

# Verify: last 64 bytes are the signature
$sigBytes = $raw[-64..-1]
$sigBytes.Length -eq 64
# → True

# Verify: payload is valid zstd (decompresses to JSON)
$payload = $raw[0..($raw.Length - 65)]
# Decompress with external tool:
#   python3 -c "import zstandard; import sys; sys.stdout.buffer.write(zstandard.decompress(open(sys.argv[1],'rb').read()))" "$cassette" | head -c 200
# Should start with: {"header":{"schema_version":1,"slug":"...
```

### 12.3 Acceptance Test: Zero-LLM Replay Guarantee

The replay engine must never call an LLM provider. To verify:

1. **Static guarantee:** The `CassetteReplayer::replay()` method imports
   no LLM provider traits and makes no HTTP calls to external APIs.
2. **Test guarantee:** `replay_zero_llm_calls` test mocks the browser
   adapter and asserts no LLM-like strings appear in the event log.
3. **Runtime guarantee:** When running `genatp cassette replay`, the
   CLI output explicitly states "Zero LLM calls" and no API keys are
   needed.

### 12.4 Acceptance Test: Hash Chain Integrity

The cassette hash chain must detect any tampering:

```bash
# Extract a cassette's step hashes:
cargo run --release -p genatp-cli -- cassette inspect example-verify-title
# Look for the hash chain verification:
#   Hash chain:      VALID ✓

# The automated test verifies this in code:
#   cargo test -p genatp-cassette --test e2e_phase7 e2e_hash_chain_integrity
```

### 12.5 Acceptance Test: Cross-Platform Compatibility

| Platform | Tests | Chrome | Keychain |
|----------|-------|--------|----------|
| Linux x86_64 | All 235 | System Chromium | GNOME Keyring / KWallet |
| macOS arm64 | All 235 | Auto-detected | Keychain Access |
| Windows x86_64 | All 235 | `%CHROME_PATH%` | Credential Manager |

Run the full suite on each platform before tagging a release. Known
differences:
- Linux: Headless Chrome works without display; `--headed` requires X/Wayland
- macOS: Keychain prompts on first vault use (choose "Always Allow")
- Windows: Long path support may be needed for deep cassette paths

---

## Appendix A: Quick Command Reference

| Task | Command |
|------|---------|
| Build debug | `cargo build -p genatp-cli` |
| Build release | `cargo build --release -p genatp-cli` |
| Run all tests | `cargo test --workspace` |
| Run cassette tests | `cargo test -p genatp-cassette` |
| Run CLI tests | `cargo test -p genatp-cli` |
| Run vault tests | `cargo test -p genatp-vault` |
| Serve stdio | `cargo run -p genatp-cli -- serve --stdio` |
| Serve HTTP | `cargo run -p genatp-cli -- serve --port 3000` |
| Serve headed | `cargo run -p genatp-cli -- serve --port 3000 --headed` |
| List vault | `cargo run -p genatp-cli -- vault list` |
| Revoke vault | `cargo run -p genatp-cli -- vault revoke <domain>` |
| List cassettes | `cargo run -p genatp-cli -- cassette list` |
| Inspect cassette | `cargo run -p genatp-cli -- cassette inspect <slug>` |
| Replay cassette | `cargo run -p genatp-cli -- cassette replay <slug>` |
| Bootstrap example | `cargo run --example bootstrap_cassette -p genatp-cassette` |
| Replay debugger | `cargo run -p genatp-cli -- replay <run-id>` |
| Start debugger API | `cargo run -p genatp-cli -- replay --serve --port 3001` |
| Search registry | `cargo run -p genatp-cli -- registry search <query>` |
| Install from registry | `cargo run -p genatp-cli -- registry install <slug>` |
| Publish to registry | `cargo run -p genatp-cli -- registry publish <path> <publisher>` |
| Start registry API | `cargo run -p genatp-cli -- registry serve --port 3002` |
| Registry info | `cargo run -p genatp-cli -- registry info <slug>` |
| List publishers | `cargo run -p genatp-cli -- registry publishers` |
| Vault reset (Windows) | `Remove-Item "$env:APPDATA\genatp\genatp\data\vault.sqlite"` (see §6.2) |
| Vault reset (macOS) | `rm ~/Library/Application\ Support/genatp/genatp/data/vault.sqlite` |
| Vault reset (Linux) | `rm ~/.local/share/genatp/genatp/data/vault.sqlite` |

## Appendix B: Test Fixture Sites

| URL | Purpose | Requires Auth |
|-----|---------|---------------|
| `https://example.com` | Basic navigation, snapshot | No |
| `https://www.automationtesting.co.uk/contactForm.html` | Form fill + submit | No |
| `https://github.com` | Real-world navigation, snapshot | No |
| `https://github.com/login` | Login form (for vault tests) | Yes |

---

*This guide is a living document. Update it when new phases ship,
when CLI flags change, or when new failure modes are discovered.*
