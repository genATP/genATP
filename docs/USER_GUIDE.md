# genATP v0.7.1 — User Guide

> **genATP** (Agent-Grade Browser Automation) lets you drive a real Chrome
> browser with an LLM agent. It compresses accessibility trees into ~400–800
> tokens per snapshot, auto-switches to vision mode on canvas/shadow-DOM
> pages, and supports MCP (Model Context Protocol) for Claude Desktop and Cursor.

---

## Quick Start (MCP Server — Recommended)

The fastest way to use genATP on real websites is through the MCP server,
which exposes browser control tools to any MCP-compatible client (Claude
Desktop, Cursor, VS Code via Continue, etc.).

### 1. Install

```bash
# Build from source (requires Rust 1.95+)
git clone https://github.com/genATP/genATP.git
cd genATP
cargo build --release -p genatp-cli

# The binary is at: target/release/genatp
# Add it to your PATH:
cp target/release/genatp ~/.local/bin/   # Linux/macOS
# or manually copy target/release/genatp.exe to a folder in your PATH on Windows
```

### 2. Set up an LLM Provider

genATP needs an LLM to interpret snapshots and decide which actions to take.
Set one of these environment variables:

| Provider   | Env var             | Model (optional)                        |
|------------|---------------------|----------------------------------------|
| Anthropic  | `ANTHROPIC_API_KEY` | `ANTHROPIC_MODEL` (default: `claude-sonnet-4-20250514`) |
| OpenAI     | `OPENAI_API_KEY`    | `OPENAI_MODEL` (default: `gpt-4o`)     |
| Google     | `GOOGLE_API_KEY`    | `GOOGLE_MODEL` (default: `gemini-2.0-flash-001`) |
| NVIDIA NIM | `NVIDIA_API_KEY`    | `NVIDIA_MODEL` (free tier available)   |
| Ollama     | —                   | Set `OLLAMA_MODEL` + start local server |

```bash
# Example: use Claude
export ANTHROPIC_API_KEY=sk-ant-...
export ANTHROPIC_MODEL=claude-sonnet-4-20250514
```

### 3. Set up Vision Provider (Optional — for canvas/shadow-DOM pages)

For pages that render inside `<canvas>` elements or use heavy shadow-DOM
(Google Maps, Figma, Jupyter Notebooks, many modern web apps), genATP
auto-detects the sparse accessibility tree and falls back to vision mode.

Set one of these for vision support:

```bash
export ANTHROPIC_API_KEY=sk-ant-...    # Claude vision (best)
# or
export OPENAI_API_KEY=sk-proj-...       # GPT-4o vision
# or
export GOOGLE_API_KEY=...               # Gemini vision
```

Priority order: Claude → GPT-4o → Gemini. If none is set, DOM mode is
always used (canvas pages will still work, but with limited information).

### 4. Start the MCP Server

```bash
# Default: stdio transport (for Claude Desktop / Cursor)
genatp serve

# Or with a visible browser window (great for debugging):
genatp serve --headed

# Or on an HTTP port (for custom clients):
genatp serve --port 3000
```

### 5. Configure Claude Desktop

Add to your Claude Desktop config (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "genatp": {
      "command": "/path/to/genatp",
      "args": ["serve"]
    }
  }
}
```

Now Claude can browse the web, fill forms, log into sites, and navigate
complex web applications.

---

## Using genATP with Claude Desktop — Step-by-Step

### Basic Web Interaction

1. **Ask Claude to visit a website:**
   > "Go to github.com/login and sign in with my credentials."

2. Claude will call `navigate("https://github.com/login")`, then
   `snapshot()` to see the page, then `fill()` to enter your username
   and password, and `click()` to submit.

3. **The workflow is always:**
   ```
   navigate(url) → snapshot() → click/fill/extract/scroll → snapshot() → ...
   ```

### Working with Canvas Pages (e.g. Google Maps, Jupyter)

When genATP detects a page with very few accessible elements (a `<canvas>`
with no interactive children), it automatically switches to **vision mode**:

1. Takes a screenshot
2. Sends it to your configured vision provider
3. Returns a description with detected interactive regions
4. You can then use `click_coordinate(x, y)` to click at specific positions

Example:
> "Show me the Eiffel Tower on Google Maps and zoom in twice."

genATP will navigate to maps.google.com, detect the canvas page, switch to
vision mode, find the search box via screenshot analysis, and interact with
coordinates.

### Authenticated Sessions (Auth Vault)

genATP includes an encrypted vault for storing cookies and localStorage.
This lets you log into a site once and reuse the session later.

**First time — save auth:**
> "Log into github.com and save the auth state."

Claude will:
1. Navigate to github.com/login
2. Fill in your credentials and click Sign In
3. Call `save_auth(domain="github.com")` to store cookies + localStorage

**Second time — load auth:**
> "Check my github notifications."

Claude will:
1. Navigate to github.com
2. Call `load_auth(domain="github.com")` to hydrate cookies
3. Navigate to github.com/notifications

### Managing the Vault

```bash
# List stored domains
genatp vault list

# Revoke a domain's auth data
genatp vault revoke github.com

# See the vault file path
genatp vault path
```

---

## Using genATP as an Orchestrator (Programmatic)

You can embed genATP directly in Rust applications:

```rust,no_run
use std::sync::Arc;
use genatp_core::{
    event_log::EventLog,
    orchestrator::{OrchestratorConfig, RunOutcome, run},
    page_adapter::PageAdapter,
    task::Task,
};
use genatp_perception::PolicyEngine;
use genatp_browser::{launch, LaunchOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Launch Chrome
    let browser = launch(LaunchOptions::default()).await?;
    let page = browser.new_page().await?;

    // Create a task
    let task = Task::new(
        "my-task",
        "Search for 'Rust programming' on example.com and return the top result.",
        "https://example.com",
        "claude-sonnet-4-20250514",
    );

    // Run the orchestrator
    let mut log = EventLog::in_memory();
    let outcome = run(OrchestratorConfig {
        task,
        browser: PageAdapter::new(page),
        provider: Arc::new(/* your provider */),
        event_log: &mut log,
        snapshot_store: None,
        policy_engine: PolicyEngine::default(),
        vision_provider: None, // or Some(Box::new(ClaudeVisionProvider::from_env()?)),
    })
    .await?;

    match outcome {
        RunOutcome::Success { summary, steps } => {
            println!("SUCCESS after {steps} steps: {summary}");
        }
        RunOutcome::Aborted { reason, steps } => {
            println!("ABORTED after {steps} steps: {reason}");
        }
    }

    browser.shutdown(std::time::Duration::from_secs(5)).await?;
    Ok(())
}
```

---

## Available MCP Tools (13 tools)

| Tool               | Purpose                                                  |
|--------------------|----------------------------------------------------------|
| `navigate`         | Navigate to a URL                                        |
| `click_coordinate` | Click at viewport coordinates (vision mode)              |
| `snapshot`         | Capture compressed accessibility tree                    |
| `click`            | Click element by `@eN` ref                               |
| `fill`             | Type into text input by `@eN` ref                        |
| `extract`          | Read visible text from the page                          |
| `scroll`           | Scroll the viewport by pixel deltas                      |
| `screenshot`       | Capture a PNG screenshot (base64)                        |
| `get_url`          | Return the current page URL                              |
| `go_back`          | Navigate back in history                                 |
| `save_auth`        | Save cookies + localStorage to encrypted vault           |
| `load_auth`        | Load cookies + localStorage from vault                   |
| `list_auth`        | List domains stored in the vault                         |

---

## Real-World Scenarios

### Scenario 1: E-Commerce Price Check

> "Go to amazon.com, search for 'wireless mouse', sort by price ascending,
> and tell me the cheapest option."

genATP will:
1. Navigate to amazon.com
2. Use `snapshot()` + `fill()` to enter search query
3. Use `snapshot()` + `click()` to press Search
4. Use `snapshot()` + `click()` to sort by price
5. Use `extract(query="price")` to read prices
6. Report the results

### Scenario 2: Canvas-Based App (Figma / Google Maps)

> "Open Google Maps and find directions from Central Park to Times Square."

For map-based pages, genATP auto-detects the `<canvas>` element and
switches to **vision mode** — it takes a screenshot and uses your vision
provider to understand the page layout before clicking coordinates.

### Scenario 3: Multi-Step Login + Data Extraction

> "Log into my admin dashboard at admin.example.com at `/login`, navigate
> to the reports page at `/reports`, download the CSV for last month, and
> save the auth state for next time."

genATP will:
1. Navigate to the login page
2. Fill credentials and click Login
3. Call `save_auth(domain="admin.example.com")`
4. Navigate to the reports page
5. Interact with the CSV download button

### Scenario 4: Record and Replay

genATP can record successful agent runs as signed cassettes and replay them
deterministically (zero LLM cost):

```bash
# Record a run
genatp record stripe-checkout \
  https://stripe.com/checkout \
  "Complete a checkout with a test card" \
  --model claude-sonnet-4-20250514

# List recorded cassettes
genatp cassette list

# Inspect a cassette
genatp cassette inspect stripe-checkout

# Replay a cassette (no LLM calls!)
genatp cassette replay stripe-checkout
```

---

## Configuration Reference

### Environment Variables

| Variable               | Purpose                                    | Default                     |
|------------------------|--------------------------------------------|-----------------------------|
| `ANTHROPIC_API_KEY`    | LLM provider (Claude)                      | —                           |
| `OPENAI_API_KEY`       | LLM provider (GPT-4o / GPT-4)             | —                           |
| `NVIDIA_API_KEY`       | LLM provider (NVIDIA NIM)                  | —                           |
| `GOOGLE_API_KEY`       | LLM provider (Gemini)                      | —                           |
| `ANTHROPIC_MODEL`      | Anthropic model override                   | `claude-sonnet-4-20250514`  |
| `OPENAI_MODEL`         | OpenAI model override                      | `gpt-4o`                    |
| `GOOGLE_MODEL`         | Google model override                      | `gemini-2.0-flash-001`      |
| `GENATP_USER_DATA_DIR` | Persistent Chrome profile path             | temp directory              |
| `GENATP_LIVE`          | Enable live E2E tests                      | —                           |
| `OLLAMA_MODEL`         | Ollama model (local inference)             | —                           |

### CLI Commands

```text
genatp v0.7.1 — agent-grade browser automation

Usage: genatp [OPTIONS] [COMMAND]

Commands:
  version   Print version and exit
  serve     Start an MCP server (stdio or HTTP)
  vault     Manage the encrypted auth vault
  record    Record a successful agent run as a signed cassette
  cassette  Manage cassettes (list, inspect, replay)
  atp       Parse, validate, and list ATP specs
  help      Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...  Verbose logging (-v for INFO, -vv for DEBUG)
  -h, --help        Print help
  -V, --version     Print version
```

---

## Troubleshooting

### "Chrome not found"
- genATP uses your system's Google Chrome or Chromium
- Ensure Chrome is installed and available in your PATH
- On Windows, check `C:\Program Files\Google\Chrome\Application\chrome.exe`

### "No LLM provider configured"
- Set one of: `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, or `GOOGLE_API_KEY`
- Or run a local Ollama server with `OLLAMA_MODEL` set

### MCP Server won't start
- Check that Chrome is installed
- Use `genatp -vv serve` for debug logs
- Check that no other Chrome instances are locking the user data directory

### Vision mode not activating on canvas pages
- Ensure a vision provider is configured (API key for Claude/GPT-4o/Gemini)
- Some canvas apps inject a minimal accessibility tree that fools the
  auto-detect — you can still use `screenshot` + manual `click_coordinate`

### "Ref not in current snapshot" errors
- Always call `snapshot()` after `navigate()` before trying to click/fill
- The page may have changed — take a fresh snapshot

---

## Further Reading

- [Architecture Overview](ARCHITECTURE.md)
- [Development Plan](DEVELOPMENT_PLAN.md)
- [Configuration Guide](CONFIGURATION.md)
- [Testing Guide](TESTING.md)
- [Vault Documentation](VAULT.md)
- [MCP Protocol Reference](MCP.md)
