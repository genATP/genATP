# genATP v1.0.0 — Complete Manual Testing Guide (Windows)

> **Audience:** QA testers and developers performing end-to-end validation.
> **Environment:** Windows (PowerShell), Chrome installed, NVIDIA NIM API key.
> **Last updated:** 2026-06-22

## Table of Contents

1. [Prerequisites & Setup](#1-prerequisites--setup)
2. [Build the Binary](#2-build-the-binary)
3. [CLI Smoke Tests (no Chrome, no API key)](#3-cli-smoke-tests)
4. [MCP Server — HTTP Transport Basics](#4-mcp-server--http-transport-basics)
5. [Scenario 1: Basic Web Navigation & Snapshot](#5-scenario-1-basic-web-navigation--snapshot)
6. [Scenario 2: Form Filling & Clicking](#6-scenario-2-form-filling--clicking)
7. [Scenario 3: Real E-Commerce Search](#7-scenario-3-real-e-commerce-search)
8. [Scenario 4: Auth Vault Lifecycle](#8-scenario-4-auth-vault-lifecycle)
9. [Scenario 5: Cassette Recording Pipeline](#9-scenario-5-cassette-recording-pipeline)
10. [Scenario 6: Cassette Replay (Zero LLM)](#10-scenario-6-cassette-replay-zero-llm)
11. [Scenario 7: Time-Travel Debugger](#11-scenario-7-time-travel-debugger)
12. [Scenario 8: Cassette Registry](#12-scenario-8-cassette-registry)
13. [Scenario 9: Error Handling & Edge Cases](#13-scenario-9-error-handling--edge-cases)
14. [Automated Test Suite](#14-automated-test-suite)
15. [Troubleshooting](#15-troubleshooting)

---

## 1. Prerequisites & Setup

### 1.1 What you need

| Requirement | Status | How to verify |
|-------------|--------|---------------|
| Windows 10/11 | ✅ Your machine | — |
| Git for Windows | ⚠️ Check | `git --version` |
| Rust toolchain (1.95+) | ⚠️ Check | `cargo --version` |
| Google Chrome | ⚠️ Check | Check `C:\Program Files\Google\Chrome\Application\chrome.exe` |
| NVIDIA NIM API key | ⚠️ Check | You said it's in place |
| PowerShell 5.1+ | ✅ Built into Windows | `$PSVersionTable.PSVersion` |

### 1.2 Set up environment variables

Open a **PowerShell** window and set these variables:

```powershell
# REQUIRED: Your NVIDIA API key (already in place)
$env:NVIDIA_API_KEY = "nvapi-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"

# RECOMMENDED: Point to NVIDIA's OpenAI-compatible endpoint
$env:OPENAI_BASE_URL = "https://integrate.api.nvidia.com"

# RECOMMENDED: Choose a model that supports tool calling
$env:GENATP_LLM_PROVIDER = "nim"
$env:GENATP_LLM_MODEL = "stepfun-ai/step-3.5-flash"

# OPTIONAL: Point to your Chrome if auto-detection fails
$env:CHROME_PATH = "C:\Program Files\Google\Chrome\Application\chrome.exe"

# OPTIONAL: Enable verbose logging to see what's happening
$env:RUST_LOG = "info"
```

> **Note:** These environment variables only last for the current PowerShell session.
> You'll need to set them again if you close and reopen the terminal.

### 1.3 Verify Chrome is findable

```powershell
# Check if Chrome exists at the standard location
Test-Path "C:\Program Files\Google\Chrome\Application\chrome.exe"
# Expected: True

# If False, find your Chrome path:
Get-ChildItem -Path "C:\Program Files\Google\Chrome" -Recurse -Filter "chrome.exe" | Select-Object -First 1 FullName
```

---

## 2. Build the Binary

### 2.1 Build release binary (first time only — takes 5–10 minutes)

```powershell
cd E:\genATP\genATP
cargo build --release -p genatp-cli
# Wait for: Finished release profile [optimized] target(s) in Xm Ys
```

### 2.2 Verify the binary works

```powershell
# Check version
.\target\release\genatp.exe version
# Expected output: genatp 1.0.0

# Check help
.\target\release\genatp.exe --help
# Expected: Shows all 10 commands listed
```

> **Note:** If you already have `target/release/genatp.exe`, you can skip step 2.1
> unless you've made code changes. To check if the binary is up to date:
> ```powershell
> ls target/release/genatp.exe
> # Should show a file from today
> ```

### 2.3 Optional: Add to PATH for convenience

```powershell
$env:Path += ";$pwd\target\release"
# Now you can run: genatp version
```

---

## 3. CLI Smoke Tests

These tests require **no Chrome, no API key, no internet**. They verify the binary
parses commands and exits cleanly.

### 3.1 No arguments

```powershell
.\target\release\genatp.exe
# Expected: "genatp 1.0.0 — try `genatp --help`"
```

### 3.2 Help per subcommand

```powershell
.\target\release\genatp.exe serve --help
.\target\release\genatp.exe vault --help
.\target\release\genatp.exe record --help
.\target\release\genatp.exe cassette --help
.\target\release\genatp.exe atp --help
.\target\release\genatp.exe replay --help
.\target\release\genatp.exe registry --help
```

Each should show the subcommand-specific arguments and flags. **Exit code: 0**.

### 3.3 Vault path

```powershell
.\target\release\genatp.exe vault path
# Expected: A Windows path like:
# C:\Users\<YOUR_USERNAME>\AppData\Roaming\genatp\genatp\data\vault.sqlite
```

### 3.4 Vault list (empty)

```powershell
.\target\release\genatp.exe vault list
# Expected: "vault not initialised yet — log in via an agent first"
```

### 3.5 Cassette list (empty)

```powershell
.\target\release\genatp.exe cassette list
# Expected: "no cassettes recorded yet"
```

### 3.6 Registry search (empty)

```powershell
.\target\release\genatp.exe registry search "test"
# Expected: "No cassettes found matching \"test\""
```

### 3.7 Registry info (non-existent)

```powershell
.\target\release\genatp.exe registry info non-existent
# Expected: Error message — "cassette non-existent matching latest not found"
```

### 3.8 ATP spec parse (non-existent file)

```powershell
.\target\release\genatp.exe atp parse non-existent.yml
# Expected: Error message about file not found
```

### 3.9 Replay (no runs exist)

```powershell
.\target\release\genatp.exe replay non-existent-run
# Expected: "runs directory not found at ..."
```

**✅ If all 9 smoke tests pass without crash, the binary is working.**

---

## 4. MCP Server — HTTP Transport Basics

The MCP (Model Context Protocol) server is the core of genATP. It exposes
browser control tools over HTTP so you can drive Chrome manually.

### 4.1 Start the server (headed mode — you'll see Chrome)

**Open Terminal A (PowerShell):**

```powershell
cd E:\genATP\genATP

# Ensure env vars are set (re-run from step 1.2 if new terminal)
$env:NVIDIA_API_KEY = "nvapi-your-key-here"
$env:OPENAI_BASE_URL = "https://integrate.api.nvidia.com"
$env:GENATP_LLM_PROVIDER = "nim"
$env:GENATP_LLM_MODEL = "stepfun-ai/step-3.5-flash"
$env:RUST_LOG = "info"

# Start the MCP server on port 3000 with a visible Chrome window
.\target\release\genatp.exe serve --port 3000 --headed -vv
```

**Expected output (stderr):**
```
INFO genatp_mcp::http: starting MCP HTTP server addr=127.0.0.1:3000
```

> ⚠️ **Chrome does NOT appear yet.** It's lazy-launched — it only opens
> when you send the first `navigate` command. Keep this terminal open.

### 4.2 Test the server with health check

**Open Terminal B (PowerShell) — keep Terminal A running:**

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:3000/healthz
# Expected: (empty 200 OK response — no error means success)
```

### 4.3 Initialize the MCP connection (required first step)

```powershell
# Step 1: Initialize
$initBody = @'
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"powershell-test","version":"1"}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body $initBody | ConvertTo-Json -Depth 10
```

**Expected:** A JSON response with `serverInfo.name: "genatp"`.

### 4.4 Send initialized notification (REQUIRED — no response)

```powershell
$notifBody = @'
{"jsonrpc":"2.0","method":"notifications/initialized"}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body $notifBody | Out-Null
# No output expected
```

### 4.5 List all available tools

```powershell
$listBody = @'
{"jsonrpc":"2.0","id":2,"method":"tools/list"}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body $listBody | ConvertTo-Json -Depth 10
```

**Expected:** A JSON object with a `tools` array containing tool descriptors.
Verify these tools exist (at minimum 12 tools):

| Tool | Purpose |
|------|---------|
| `navigate` | Navigate to a URL |
| `snapshot` | Capture compressed accessibility tree |
| `click` | Click element by `@eN` ref |
| `fill` | Type into text input by `@eN` ref |
| `extract` | Read visible text from the page |
| `scroll` | Scroll the viewport |
| `screenshot` | Capture PNG screenshot (base64) |
| `get_url` | Return current page URL |
| `go_back` | Navigate back in history |
| `save_auth` | Save cookies + localStorage to vault |
| `load_auth` | Load cookies + localStorage from vault |
| `list_auth` | List domains stored in the vault |

**✅ If all 12 tools are present, the MCP server is correctly wired.**

---

## 5. Scenario 1: Basic Web Navigation & Snapshot

**Goal:** Verify genATP can navigate to a real website, take a snapshot,
extract text, and read the URL.

Keep the server running from Terminal A. In Terminal B:

### 5.1 Navigate to example.com

```powershell
$navigateBody = @'
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://example.com"}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body $navigateBody | ConvertTo-Json -Depth 10
```

**Expected:** `"isError": false` with `"text": "navigated to https://example.com/"`.

> 🔥 **Chrome appears NOW** — a visible browser window opens and navigates
> to example.com. This is the first browser command.

### 5.2 Take a snapshot (accessibility tree)

```powershell
$snapshotBody = @'
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"snapshot"}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body $snapshotBody | ConvertTo-Json -Depth 10
```

**Expected:** A JSON response with compressed accessibility tree. You should
see `@e1`, `@e2`, etc. refs. The tree should contain elements like:
- A link to "More information..." (`@e1` or similar)
- The text "This domain is for use in illustrative examples"

**What to verify:**
- ✅ Snapshot returns valid JSON
- ✅ Ref references (`@eN`) are present
- ✅ Page content is recognizable
- ✅ Response time is under 1 second

### 5.3 Get current URL

```powershell
$urlBody = @'
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"get_url"}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body $urlBody
```

**Expected:** `"text": "https://example.com/"`

### 5.4 Extract visible text

```powershell
$extractBody = @'
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"extract","arguments":{"query":""}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body $extractBody
```

**Expected:** Visible text content from the page including
"This domain is for use in illustrative examples in documents."

### 5.5 Take a screenshot

```powershell
$screenshotBody = @'
{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"screenshot"}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body $screenshotBody
```

**Expected:** A JSON response with a `data` field containing a long
base64-encoded string (the PNG image). The string should start with
something like `/9j/4AAQ...` or similar base64 image data.

> **To view the screenshot:** Copy the base64 data and use an online
> base64-to-image decoder, or save it to a file:
> ```powershell
> # Extract the base64 data (the text field after "text":)
> $result = Invoke-RestMethod ...
> $base64 = $result.content[0].text
> [System.IO.File]::WriteAllBytes("$env:TEMP\screenshot.png", [Convert]::FromBase64String($base64))
> # Then open: start $env:TEMP\screenshot.png
> ```

### 5.6 Scroll the page

```powershell
$scrollBody = @'
{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"scroll","arguments":{"dx":0,"dy":300}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body $scrollBody
```

**Expected:** `"isError": false`

### 5.7 Go back (should say "no prior page" since we just landed)

```powershell
$backBody = @'
{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"go_back"}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body $backBody
```

**Expected:** `"text": "no prior page"`

**✅ Scenario 1 PASS if:** Navigation → Snapshot → Extract → Screenshot
all return non-error responses with the expected content.

---

## 6. Scenario 2: Form Filling & Clicking

**Goal:** Verify genATP can interact with HTML forms — fill text fields,
click buttons, and handle form submission.

### 6.1 Navigate to the contact form

```powershell
$navBody = @'
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://www.automationtesting.co.uk/contactForm.html"}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body $navBody | ConvertTo-Json -Depth 10
```

**Expected:** `"text": "navigated to https://www.automationtesting.co.uk/contactForm.html"`

### 6.2 Take a snapshot to find form field refs

```powershell
$snapBody = @'
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"snapshot"}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" `
  -Body $snapBody | ConvertTo-Json -Depth 10
```

**Critical:** Look at the response and find the `@eN` refs for each form field.
The form has these fields (ref numbers will differ — use what the snapshot returns):
- First Name (text input)
- Last Name (text input)
- Email Address (text input)
- Comments (textarea)
- SUBMIT button

Write down the actual `@eN` refs from your snapshot. For example:
- First Name = `@e442`
- Last Name = `@e443`
- Email = `@e444`
- Comments = `@e445`
- SUBMIT = `@e448`

### 6.3 Fill in each form field

**Replace `@eXXX` with the actual refs from your snapshot:**

```powershell
# Fill First Name
$body = @'
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"fill","arguments":{"ref":"@e442","value":"John"}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body

# Fill Last Name
$body = @'
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"fill","arguments":{"ref":"@e443","value":"Doe"}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body

# Fill Email
$body = @'
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"fill","arguments":{"ref":"@e444","value":"john@example.com"}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body

# Fill Comments
$body = @'
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"fill","arguments":{"ref":"@e445","value":"This is a test message."}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body
```

**Each `fill` should return:** `"isError": false`

### 6.4 Click the SUBMIT button

```powershell
$body = @'
{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"click","arguments":{"ref":"@e448"}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body
```

**Expected:** The form submits and the page navigates to the thank-you page.
The response should indicate success. **Look at the Chrome window** — it
should show "Thank you for your mail!"

### 6.5 Verify the form submission

```powershell
# Take a snapshot to confirm
$body = @'
{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"snapshot"}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body | ConvertTo-Json -Depth 10
```

**Expected:** Snapshot contains "Thank you for your mail!" text.

**✅ Scenario 2 PASS if:** All fields filled, submit clicked, and
thank-you page confirmed.

---

## 7. Scenario 3: Real E-Commerce Search

**Goal:** Verify genATP can navigate a real e-commerce site and extract
structured information.

### 7.1 Navigate to GitHub

```powershell
$body = @'
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://github.com"}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body | ConvertTo-Json -Depth 10
```

### 7.2 Snapshot the GitHub homepage

```powershell
$body = @'
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"snapshot"}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body | ConvertTo-Json -Depth 10
```

**Verify:** The snapshot should contain recognizable GitHub elements —
navigation links, "Sign in" button, search input, etc. Look for refs
like `@e1` through `@e20+`.

### 7.3 Extract visible text (searchable)

```powershell
$body = @'
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"extract"}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body
```

**Verify:** Text should mention "GitHub", "Let's build from here", etc.

### 7.4 Go back (should say "no prior page" since we came from another domain)

```powershell
$body = @'
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"go_back"}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body
```

**Expected:** Either `"navigated back"` or `"no prior page"`.

**✅ Scenario 3 PASS if:** GitHub loads, snapshot reveals structured
navigation elements, text extraction returns readable content.

---

## 8. Scenario 4: Auth Vault Lifecycle

**Goal:** Verify the encrypted auth vault can save, list, and revoke
credentials.

### 8.1 Check vault status (CLI, in Terminal B)

```powershell
.\target\release\genatp.exe vault list
# Expected: "vault not initialised yet — log in via an agent first"
```

### 8.2 Navigate to a site (via existing server on Terminal A)

```powershell
$body = @'
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://www.automationtesting.co.uk/contactForm.html"}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body | ConvertTo-Json -Depth 10
```

### 8.3 Save auth for the domain

```powershell
$body = @'
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"save_auth","arguments":{"domain":"automationtesting.co.uk"}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body
```

**Expected:** `"text": "saved N auth entries for automationtesting.co.uk"`
(where N might be 0 for a site without login cookies — that's OK).

### 8.4 List auth via CLI

```powershell
.\target\release\genatp.exe vault list
```

**Expected:** A table showing any saved domains.

### 8.5 List auth via MCP tool

```powershell
$body = @'
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_auth"}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body
```

**Expected:** Lists any saved domains.

### 8.6 Revoke the domain

```powershell
.\target\release\genatp.exe vault revoke automationtesting.co.uk
# Expected: "revoked all entries for automationtesting.co.uk"
```

### 8.7 Confirm revocation

```powershell
.\target\release\genatp.exe vault list
# Expected: The domain no longer appears (or vault is empty)
```

**✅ Scenario 4 PASS if:** Save → List → Revoke → Confirm all work correctly.

---

## 9. Scenario 5: Cassette Recording Pipeline

**Goal:** Verify the cassette recording and storage pipeline works.

### 9.1 Generate a sample cassette

```powershell
cd E:\genATP\genATP
.\target\release\genatp.exe cassette list
# Expected: "no cassettes recorded yet"

# Run the bootstrap example to create a real cassette on disk
cargo run --release --example bootstrap_cassette -p genatp-cassette
# Expected: "Cassette bootstrap complete" or similar
```

### 9.2 List cassettes (should show the newly created one)

```powershell
.\target\release\genatp.exe cassette list
# Expected:
# slug                                        versions
# example-verify-title                        1.0.0
```

### 9.3 Inspect the cassette

```powershell
.\target\release\genatp.exe cassette inspect example-verify-title
```

**Expected:** Full metadata output including:
- Schema version
- Goal: "Verify the page title at https://example.com"
- Start URL: https://example.com
- Signature: VALID ✓
- Hash chain: VALID ✓
- Steps listing

### 9.4 Inspect a specific version

```powershell
.\target\release\genatp.exe cassette inspect example-verify-title --version 1.0.0
# Expected: Same as above, explicitly showing v1.0.0
```

### 9.5 Verify tamper detection

**Find the cassette file:**

```powershell
# Use vault path to find the data directory
$vaultPath = .\target\release\genatp.exe vault path 2>&1
# Or check the standard location:
$cassetteDir = "$env:APPDATA\genatp\genatp\data\cassettes\example-verify-title\versions"
Write-Output "Checking: $cassetteDir"
Get-ChildItem $cassetteDir
```

**Tamper with the file:**

```powershell
$cassetteFile = "$cassetteDir\1.0.0.cassette"
# Verify it exists
Test-Path $cassetteFile

# Create backup
Copy-Item $cassetteFile "$cassetteFile.backup"

# Flip a byte to corrupt the signature
$raw = [System.IO.File]::ReadAllBytes($cassetteFile)
$raw[$raw.Length / 2 - 1] = $raw[$raw.Length / 2 - 1] -bxor 0xFF
[System.IO.File]::WriteAllBytes($cassetteFile, $raw)

# Try to inspect — should fail
.\target\release\genatp.exe cassette inspect example-verify-title
# Expected: "signature invalid" or similar error

# Restore backup
Copy-Item "$cassetteFile.backup" $cassetteFile -Force

# Verify it works again
.\target\release\genatp.exe cassette inspect example-verify-title
# Expected: "Signature: VALID ✓"
```

**✅ Scenario 5 PASS if:** Cassette creates, lists, inspects, tamper
detection works, and restore succeeds.

---

## 10. Scenario 6: Cassette Replay (Zero LLM)

**Goal:** Verify cassette replay works without any LLM calls.

### 10.1 Dry-run replay the cassette

```powershell
.\target\release\genatp.exe cassette replay example-verify-title
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

  [ 0] ✓ Navigate { url: "https://example.com" }

  Dry-run: 1 would succeed, 0 would need a ref resolver
```

### 10.2 Replay a specific version

```powershell
.\target\release\genatp.exe cassette replay example-verify-title --version 1.0.0
# Expected: Same output as above
```

### 10.3 Run automated replay tests

```powershell
cargo test -p genatp-cassette --test e2e_phase8_replay -- --nocapture
```

**Expected:** "3 passed; 0 failed"

**✅ Scenario 6 PASS if:** Replay runs without LLM, output confirms
zero LLM calls, automated tests pass.

---

## 11. Scenario 7: Time-Travel Debugger

**Goal:** Verify the debugger can replay past runs and the API/UI works.

### 11.1 Start the debugger API server

**Open Terminal C (PowerShell) — keep Terminal A (MCP server) running:**

```powershell
cd E:\genATP\genATP
.\target\release\genatp.exe replay --serve --port 3001
# Expected: "Starting debugger API server at http://127.0.0.1:3001"
```

### 11.2 Health check the debugger API

**In Terminal B (or Terminal D):**

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:3001/health
# Expected: {"status":"ok"}
```

### 11.3 List available runs (will be empty)

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:3001/runs
# Expected: [] (empty array)
```

### 11.4 Test with a non-existent run

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:3001/runs/non-existent
# Expected: Error response (run not found)
```

### 11.5 Replay a non-existent run via CLI

```powershell
.\target\release\genatp.exe replay non-existent-run
# Expected: Error — "runs directory not found at ..."
```

### 11.6 (Optional) Start the Web UI

```powershell
# Requires Node.js — skip if not available
cd crates/genatp-debugger/ui
npm install
npm run dev
# Opens at http://localhost:5173
```

**✅ Scenario 7 PASS if:** Debugger server starts, health check responds,
and API returns proper responses (even if empty).

---

## 12. Scenario 8: Cassette Registry

**Goal:** Verify the registry API and CLI work correctly.

### 12.1 Start the registry API server

**Open Terminal D (PowerShell) — keep Terminals A and C running:**

```powershell
cd E:\genATP\genATP
.\target\release\genatp.exe registry serve --port 3002
# Expected: "Starting registry API server at http://127.0.0.1:3002"
```

### 12.2 Health check the registry

**In Terminal B:**

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:3002/health
# Expected: {"status":"ok"}
```

### 12.3 Check OCI base endpoint

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:3002/v2/
# Expected: {"version":"1.1"}
```

### 12.4 Search for cassettes (empty registry)

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:3002/search?q=test
# Expected: [] (empty array)
```

### 12.5 List publishers (empty)

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:3002/publishers
# Expected: [] (empty array)
```

### 12.6 CLI search (empty)

```powershell
.\target\release\genatp.exe registry search "test"
# Expected: "No cassettes found matching \"test\""
```

### 12.7 CLI publishers (empty)

```powershell
.\target\release\genatp.exe registry publishers
# Expected: "No registered publishers."
```

### 12.8 CLI info (non-existent)

```powershell
.\target\release\genatp.exe registry info non-existent
# Expected: Error — "cassette non-existent matching latest not found"
```

**✅ Scenario 8 PASS if:** Registry server starts, OCI endpoint responds,
and all CLI commands handle empty state gracefully.

---

## 13. Scenario 9: Error Handling & Edge Cases

**Goal:** Verify genATP handles errors gracefully.

### 13.1 Unknown tool

With the MCP server (Terminal A) running:

```powershell
$body = @'
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nonexistent_tool","arguments":{}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body | ConvertTo-Json -Depth 10
```

**Expected:** `"isError": true` with an appropriate error message.

### 13.2 Navigate to invalid URL

```powershell
$body = @'
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"not-a-url"}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body | ConvertTo-Json -Depth 10
```

**Expected:** `"isError": true` with URL parse error.

### 13.3 Navigate to unreachable URL

```powershell
$body = @'
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://this-domain-does-not-exist-12345.com"}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body | ConvertTo-Json -Depth 10
```

**Expected:** Error or timeout message (browser can't resolve DNS).

### 13.4 Click on non-existent ref

First, take a snapshot to get valid refs, then use a ref that doesn't exist:

```powershell
$body = @'
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"click","arguments":{"ref":"@e99999"}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body | ConvertTo-Json -Depth 10
```

**Expected:** `"isError": true` with "ref @e99999 not in current snapshot" or similar.

### 13.5 Fill before snapshot

Without taking a snapshot first:

```powershell
# First navigate to a new page (invalidates refs)
$body = @'
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"navigate","arguments":{"url":"https://example.com"}}}
'@
Invoke-RestMethod ...  # (this works)

# Now try to fill WITHOUT snapshot
$body = @'
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"fill","arguments":{"ref":"@e1","value":"hello"}}}
'@
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:3000/ `
  -ContentType "application/json" -Body $body | ConvertTo-Json -Depth 10
```

**Expected:** `"isError": true` — refs are stale after navigation.

### 13.6 Vault revoke non-existent domain

```powershell
.\target\release\genatp.exe vault revoke nonexistent.com
# Expected: "no entries for nonexistent.com (already revoked or never stored)"
```

**✅ Scenario 9 PASS if:** All error cases return clear, non-cryptic errors
and the server doesn't crash.

---

## 14. Automated Test Suite

Run the full automated test suite to verify no regressions.

### 14.1 Unit + integration tests (no Chrome needed)

```powershell
cd E:\genATP\genATP

# Run all tests (excluding Python and Node bindings)
cargo test --workspace --exclude genatp-python --exclude genatp-node 2>&1 | Select-String -Pattern "test result|FAILED|^test "
```

**Expected:** All tests pass. The output should show:
```
test result: ok. X passed; 0 failed; Y ignored
```
for every crate.

### 14.2 CLI tests specifically

```powershell
cargo test -p genatp-cli 2>&1
# Expected: All CLI tests pass
```

### 14.3 Cassette tests

```powershell
cargo test -p genatp-cassette 2>&1
# Expected: All cassette tests pass
```

### 14.4 Registry tests

```powershell
cargo test -p genatp-registry 2>&1
# Expected: All registry tests pass
```

### 14.5 Vault tests

```powershell
cargo test -p genatp-vault 2>&1
# Expected: All vault tests pass
```

### 14.6 Debugger tests

```powershell
cargo test -p genatp-debugger 2>&1
# Expected: All debugger tests pass
```

### 14.7 Static analysis

```powershell
cargo clippy --workspace --exclude genatp-python --exclude genatp-node
# Expected: No warnings (0 warnings)

cargo fmt --all -- --check
# Expected: No output (all files formatted correctly)
```

---

## 15. Troubleshooting

### 15.1 Server won't start

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| `Connection refused` on health check | Server not running yet | Wait for `starting MCP HTTP server` message |
| Chrome doesn't open | `--headed` flag missing | Stop server, re-add `--headed` flag |
| `No LLM provider configured` | API key not set | Set `$env:NVIDIA_API_KEY` |
| Port already in use | Another server running | Use a different port (`--port 3001`) |
| Chrome crash on launch | Chrome path wrong | Set `$env:CHROME_PATH` explicitly |

### 15.2 Tool call errors

| Error | Fix |
|-------|-----|
| `ref @eN not in current snapshot` | Call `snapshot()` first |
| `navigate` returns error for valid URL | The page might block bots — try a different site |
| `fill` doesn't work | The element might be an `input` type that needs `click()` first |
| `save_auth` shows 0 entries | The page didn't set cookies — that's OK for non-login pages |

### 15.3 PowerShell tips

```powershell
# To see verbose server logs
$env:RUST_LOG = "debug"

# To test if the server process is running
Get-Process -Name "genatp" -ErrorAction SilentlyContinue

# To kill a stuck server
Stop-Process -Name "genatp" -Force

# To check what ports are in use
netstat -an | Select-String "3000|3001|3002"
```

### 15.4 Common Windows issues

**Long paths:** If you get "path too long" errors:
```powershell
# Run as Administrator once:
New-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" `
  -Name "LongPathsEnabled" -Value 1 -PropertyType DWORD -Force
```

**Chrome not found at default path:**
```powershell
# Find Chrome
$paths = @(
    "C:\Program Files\Google\Chrome\Application\chrome.exe",
    "C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    "$env:LOCALAPPDATA\Google\Chrome\Application\chrome.exe"
)
foreach ($p in $paths) { if (Test-Path $p) { Write-Output "Found: $p" } }

# Set it
$env:CHROME_PATH = "C:\Program Files\Google\Chrome\Application\chrome.exe"
```

**NVIDIA model doesn't respond:**
```powershell
# Try a different model
$env:GENATP_LLM_MODEL = "nvidia/nemotron-mini-4b-instruct"
```

---

## Pass Criteria Checklist

Before marking the test run as complete, verify all scenarios pass:

| # | Scenario | ✅ |
|---|----------|----|
| 1 | CLI smoke tests (9 subtests all pass) | ☐ |
| 2 | MCP server starts and lists 12 tools | ☐ |
| 3 | Navigate, snapshot, extract, screenshot work on example.com | ☐ |
| 4 | Form fill + submit on automationtesting.co.uk | ☐ |
| 5 | GitHub loads with structured snapshot | ☐ |
| 6 | Auth Vault save → list → revoke works | ☐ |
| 7 | Cassette create → list → inspect works | ☐ |
| 8 | Cassette tamper detection works | ☐ |
| 9 | Cassette replay shows zero-LLM dry-run | ☐ |
| 10 | Debugger API starts and responds | ☐ |
| 11 | Registry API starts with OCI endpoint | ☐ |
| 12 | Error handling returns clear messages | ☐ |
| 13 | Full test suite passes | ☐ |
| 14 | Clippy and formatting clean | ☐ |

---

*This guide covers all genATP v1.0.0 features. Report any failures
to the GitHub issues page with the scenario number and exact error output.*
