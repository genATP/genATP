# Configuration

genATP is configured through **environment variables**. Both local LLMs
(Ollama) and hosted APIs (Anthropic, OpenAI, NVIDIA NIM) are
first-class — pick whichever fits your workflow, no recompile required.

Status: this guide covers the **v1.0.0** self-hosted product (CLI, SDKs, MCP,
Auth Vault with OS keychain, Action Cassettes, NIM/Anthropic/OpenAI/Ollama).
Environment variables remain the source of truth for LLM providers; the vault
stores session cookies/localStorage via the OS keychain, not API keys.

---

## TL;DR

```bash
# 1. Copy the template
cp .env.example .env

# 2. Pick a provider — fill in ONE of these blocks in .env
#    (a) Local, free, no signup:                no edits needed; defaults to Ollama
#    (b) Anthropic Claude:                      ANTHROPIC_API_KEY=…
#    (c) OpenAI:                                OPENAI_API_KEY=…
#    (d) NVIDIA NIM (free hosted endpoints):    NVIDIA_API_KEY=nvapi-…

# 3. Load it into the current shell
set -a; source .env; set +a

# 4. Run anything
cargo run --release -p genatp-cli -- serve --port 3000
```

> genATP does **not** parse `.env` itself. Source it manually, or use
> [`direnv`](https://direnv.net/) (`echo dotenv > .envrc && direnv allow`)
> for auto-load on `cd`.

---

## Provider selection

There are two ways to choose a backend.

### Auto-detection (recommended)

Leave `GENATP_LLM_PROVIDER` unset. The factory walks env vars in this
order and picks the first match:

| Priority | Trigger                                                    | Backend          |
|---------:|------------------------------------------------------------|------------------|
| 1        | `ANTHROPIC_API_KEY` is set                                 | Anthropic Claude |
| 2        | `NVIDIA_API_KEY` is set, or `OPENAI_BASE_URL` is NIM       | NVIDIA NIM       |
| 3        | `OPENAI_API_KEY` is set                                    | OpenAI           |
| 4        | nothing else matched                                       | Ollama (local)   |

This means dropping a single key in `.env` is enough to switch providers.

### Explicit override

Set `GENATP_LLM_PROVIDER` to force a specific backend (handy when you
have multiple keys configured):

```bash
GENATP_LLM_PROVIDER=nim                      # one of: anthropic | openai | nim | ollama
GENATP_LLM_MODEL=mistralai/mistral-nemotron  # optional; sensible per-provider defaults
```

Aliases accepted: `claude` → anthropic, `gpt` → openai, `nvidia` → nim,
`local` → ollama. Case-insensitive.

---

## Per-provider setup

### Local: Ollama

The zero-friction path. genATP defaults to this if nothing else is
configured.

```bash
# Install (macOS)
brew install ollama
ollama serve &              # in another terminal

# Pull a model that supports tool calling
ollama pull llama3.1:8b     # also good: qwen2.5:7b, mistral-nemo
```

| Variable                  | Default                       | Purpose                |
|---------------------------|-------------------------------|------------------------|
| `OLLAMA_BASE_URL`         | `http://127.0.0.1:11434`      | Ollama daemon endpoint |
| `GENATP_LLM_MODEL`        | `llama3.1:8b`                 | Default model id       |

Avoid tiny models (`phi3`, anything &lt; 7B) — their tool-call output is
unreliable and the orchestrator will retry until the budget aborts.

### API: Anthropic Claude

```bash
ANTHROPIC_API_KEY=sk-ant-…
# Optional:
ANTHROPIC_BASE_URL=https://api.anthropic.com
GENATP_LLM_MODEL=claude-3-5-sonnet-latest
```

API key from [console.anthropic.com](https://console.anthropic.com).

### API: OpenAI

```bash
OPENAI_API_KEY=sk-…
# Optional:
OPENAI_BASE_URL=https://api.openai.com
OPENAI_ORG=org-…
GENATP_LLM_MODEL=gpt-4o-mini
```

API key from [platform.openai.com](https://platform.openai.com/api-keys).

### API: NVIDIA NIM (OpenAI-wire)

NVIDIA hosts an OpenAI-compatible endpoint at
`https://integrate.api.nvidia.com` and exposes many models for free.
genATP routes NIM through its OpenAI adapter — no separate code path.

```bash
NVIDIA_API_KEY=nvapi-…
GENATP_LLM_PROVIDER=nim                      # optional but explicit
GENATP_LLM_MODEL=qwen/qwen3-coder-480b-a35b-instruct
```

API key from [build.nvidia.com](https://build.nvidia.com) → *Get API Key*.

**Recommended models for browser/agentic work** (must support tool
calling). Live-tested on the genATP agent loop, ranked best-to-worst:

| Model id                                       | Tool calls correctly? | Free-tier latency | Notes                                      |
|------------------------------------------------|:---------------------:|:-----------------:|--------------------------------------------|
| `openai/gpt-oss-20b`                           | yes (verify)          | ~6 s              | **Verified 2026-08-31** — example.com H1 in 1 step / 1058 tok. |
| `mistralai/mistral-nemotron`                   | verify on your key    | slower            | Listed on NIM; can timeout on free tier. |
| `stepfun-ai/step-3.5-flash`                    | ~~EOL~~               | —                 | **Gone** (NIM 410 after 2026-07-27). Do not use. |
| `nvidia/nemotron-mini-4b-instruct`             | may be unavailable    | —                 | Not present on integrate.api.nvidia.com model list (2026-08-31). |
| `meta/llama-4-maverick-17b-128e-instruct`      | no — JSON-as-text     | ~27 s             | Replies, but tool format is wrong.         |
| `mistralai/mistral-nemotron`                   | no — JSON-as-text     | ~1 s              | Same problem; quick but unusable for agents. |
| `qwen/qwen3-coder-480b-a35b-instruct`          | unknown               | timed out (180 s) | Overloaded on free tier as of May 2026.   |
| `mistralai/mistral-large-3-675b-instruct-2512` | unknown               | 504 after 5 min   | Overloaded on free tier as of May 2026.   |

Do **not** point the agentic loop at the safety, embedding, vision-only,
TTS, video, or autonomous-driving NIMs — they don't speak chat-with-tools.

---

## Where to put your secrets

| Option                  | Use when…                                              | Trade-off                                              |
|-------------------------|--------------------------------------------------------|--------------------------------------------------------|
| `.env` + `source .env`  | Local development; you control the shell               | You have to remember to source it                      |
| `direnv`                | Multiple projects, you want auto-load on `cd`          | Extra tool (`brew install direnv`)                     |
| Shell rc (`~/.zshrc`)   | One personal key you use everywhere                    | Leaks into every shell session, harder to compartmentalise |
| Plain export per run    | Quick ad-hoc tests                                     | Lost on shell exit                                     |
| OS keychain (Phase 6)   | **Coming in v0.3.0** — see [DEVELOPMENT_PLAN.md](./DEVELOPMENT_PLAN.md) Phase 6 | Not available yet                                      |

**Never commit `.env`.** It's already in `.gitignore`. The
pre-commit `gitleaks` hook will catch most accidental key prefixes too.

---

## Browser configuration

| Variable               | Default                              | Purpose                                                 |
|------------------------|--------------------------------------|---------------------------------------------------------|
| `CHROME_PATH`          | (auto-detected)                      | Explicit Chrome/Chromium binary path                    |
| `GENATP_USER_DATA_DIR` | fresh temp dir per run               | Persistent profile for cookies/localStorage             |
| `RUST_LOG`             | `warn`                               | Standard `tracing-subscriber` filter (logs → stderr)    |

On macOS, Chrome is usually auto-detected at
`/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`.

---

## Testing matrix

| Test surface                                       | Needs LLM key? | Command                                                              |
|----------------------------------------------------|----------------|----------------------------------------------------------------------|
| Unit + integration (CI default)                    | No             | `cargo test --workspace`                                             |
| `form_fill_ollama` (live local LLM, opt-in)        | No (local)     | `OLLAMA_LIVE=1 cargo test -p genatp-core --test form_fill_ollama -- --ignored --nocapture` |
| Manual demo against fixture site                   | No             | `cargo run --bin genatp_browser_demo`                                |
| Live agent loop against a hosted API (ad-hoc)      | **Yes**        | Set the relevant `*_API_KEY`, then run a binary that calls `genatp_llm::resolve_from_env` |
| Phase 4 manual sign-off (Claude Desktop demo)      | Yes — but the key lives in **Claude Desktop's** config, not genATP | See [MCP.md](./MCP.md)               |

The full CI-equivalent gate (`./scripts/preflight.sh`) never touches a
live LLM. By design — see `crates/genatp-llm/src/lib.rs` module docs
and `crates/genatp-llm/src/vcr.rs`.

---

## Programmatic usage

```rust
use genatp_llm::{resolve_from_env, CompletionRequest, Message};

let provider = resolve_from_env()?;
tracing::info!(kind = ?provider.kind, model = %provider.model, "selected provider");

let request = CompletionRequest::chat(
    &provider.model,
    vec![Message::user("Say hello.")],
);
let response = provider.provider.complete(request).await?;
println!("{}", response.message.content);
```

The factory returns an `Arc<dyn LlmProvider>`, so the same value can be
shared across orchestrator tasks without re-construction.
