# genATP User Manual — v1.0.0

> Complete guide for **self-hosted** genATP: CLI, MCP, Python SDK, Node SDK,
> Auth Vault, Action Cassettes, and ATP Spec.
>
> Configuration details: [`CONFIGURATION.md`](./CONFIGURATION.md) · Architecture:
> [`ARCHITECTURE.md`](./ARCHITECTURE.md) · Sign-off runbooks:
> [`FULL_SYSTEM_USER_TEST.md`](./FULL_SYSTEM_USER_TEST.md)

---

## 1. What genATP is

**genATP** (Generative Agent Test Protocol) is a Rust browser-agent engine
shipped as one binary (`genatp`) plus language SDKs. It drives real Chrome via
CDP, compresses accessibility trees for LLMs (~400–800 tokens per snapshot),
and adds:

| Capability | Summary |
|---|---|
| **CLI** | `run`, `record`, `serve`, vault, cassettes, registry, ATP |
| **MCP server** | Browser tools for Cursor / Claude Desktop |
| **Python / Node SDKs** | `Agent.run()` with budgets, recording, ask_user |
| **Action Cassettes** | Record once → replay with **0 LLM tokens** |
| **Auth Vault** | Encrypted cookies + localStorage (OS keychain) |
| **ATP Spec** | Declarative YAML/JSON workflows |

Cloud / Enterprise tiers are **not** part of the open self-hosted codebase.

---

## 2. Install

### 2.1 CLI (from source)

```bash
git clone https://github.com/genatp/genatp && cd genatp
cargo build --release -p genatp-cli
```

**Windows:** add `target\release` (or your `--target-dir`) to PATH **before** any older `genatp.exe`.

```powershell
$env:Path = "E:\genATP\genATP\target\release;" + $env:Path
genatp version   # genatp 1.0.0
```

### 2.2 Chrome

Install Google Chrome or Chromium. Override detection:

```bash
export CHROME_PATH="/path/to/chrome"   # or set in .env
```

### 2.3 LLM provider (.env)

```bash
cp .env.example .env
# Edit ONE provider block, then source .env (genATP does not auto-load .env)
```

**NVIDIA NIM (verified):**

```bash
export NVIDIA_API_KEY=nvapi-…
export GENATP_LLM_PROVIDER=nim
export GENATP_LLM_MODEL=openai/gpt-oss-20b
```

Do **not** use `stepfun-ai/step-3.5-flash` — EOL on NIM (HTTP 410).

### 2.4 Python SDK

```bash
cd bindings/python
python -m pip install maturin
python -m maturin develop --release
python -c "import genatp; print(genatp.__version__)"   # 1.0.0
```

### 2.5 Node SDK

```bash
cd bindings/node
npm install
npx napi build --platform --release
node -e "console.log(require('./index.js').version())"   # 1.0.0
```

---

## 3. Configuration (condensed)

| Variable | Purpose |
|---|---|
| `GENATP_LLM_PROVIDER` | Force `anthropic`, `openai`, `nim`, `ollama` |
| `GENATP_LLM_MODEL` / `NVIDIA_MODEL` | Model id override |
| `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `NVIDIA_API_KEY` | Provider credentials |
| `CHROME_PATH` | Chrome binary |
| `GENATP_USER_DATA_DIR` | Persistent Chrome profile |
| `GENATP_VAULT_PATH` / `GENATP_HOME` | Vault SQLite location |
| `RUST_LOG` | `warn`, `info`, `debug` |

Full table: [`CONFIGURATION.md`](./CONFIGURATION.md).

---

## 4. CLI reference

Run `genatp --help` and `genatp <cmd> --help` on your build.

### 4.1 `genatp version`

Prints workspace version (expect **1.0.0**).

### 4.2 `genatp serve`

MCP server (stdio default).

```bash
genatp serve --stdio                    # Claude / Cursor
genatp serve --port 3000              # HTTP on 127.0.0.1
genatp serve --headed                   # visible Chrome
genatp serve --user-data-dir ~/.genatp-profile
genatp serve --vault-domains github.com,example.com
```

### 4.3 `genatp run`

Agent goal with optional cassette router + record.

```bash
genatp run \
  --goal "Extract the H1 from example.com" \
  --start-url "https://example.com" \
  --max-tokens 50000 --max-steps 20

# Record on success
genatp run --goal "…" --start-url "https://example.com" \
  --record --slug my-slug --headed

# Force LLM (skip cassette hit)
genatp run --goal "…" --start-url "…" --skip-cassette

# Isolated Chrome profile (use beside MCP)
genatp run --goal "…" --start-url "…" \
  --user-data-dir "$TEMP/genatp-cli-profile"
```

**Defaults:** `--max-tokens 100000`, `--max-wall-s 600`, `--max-steps 30`.

**Stdout:** `Outcome`, `Steps`, `Tokens`, `Summary`, optional `Cassette:` path.

Router hit example: `Outcome: success (cassette replay)`, `Tokens: 0`.

### 4.4 `genatp record`

Shorthand for agent run that always saves a cassette.

```bash
genatp record my-slug "https://example.com" "Extract the H1" --headed
```

### 4.5 `genatp vault`

```bash
genatp vault path
genatp vault list
genatp vault revoke github.com
```

Live E2E: [`AUTH_VAULT_LIVE_E2E.md`](./AUTH_VAULT_LIVE_E2E.md).

### 4.6 `genatp cassette`

```bash
genatp cassette list
genatp cassette inspect my-slug [--version 1.0.0]
genatp cassette replay my-slug [--headed] [--user-data-dir /tmp/profile]
```

Replay uses **zero LLM calls**. `--user-data-dir` avoids profile locks when MCP is running.

### 4.7 `genatp atp`

Validate declarative specs:

```bash
genatp atp validate examples/atp-spec-ci/login.atp.yml
genatp atp list .
genatp atp steps examples/atp-spec-ci/search.atp.yml
```

### 4.8 `genatp replay`

Time-travel debugger for recorded agent runs (not cassette replay):

```bash
genatp replay <run-id> [--at 3] [--serve --port 3001]
```

### 4.9 `genatp registry`

Local SQLite registry (no cloud backend):

```bash
genatp registry search "example"
genatp registry info my-slug latest
genatp registry publish ~/.genatp/cassettes/my-slug/versions/1.0.0.cassette local-dev
genatp registry publishers
genatp registry install my-slug latest --yes
genatp registry serve --port 3002
```

---

## 5. MCP

Wire into Cursor / Claude Desktop:

```json
{
  "mcpServers": {
    "genatp": {
      "command": "genatp",
      "args": ["serve", "--stdio", "--headed"],
      "env": { "RUST_LOG": "warn" }
    }
  }
}
```

### Tools

| Tool | Purpose |
|---|---|
| `navigate` | Go to URL |
| `snapshot` | Compressed a11y tree → `@eN` refs |
| `click` / `fill` | Target `@eN` from latest snapshot |
| `extract` | Visible text filter |
| `scroll` | Scroll page |
| `screenshot` | PNG capture |
| `get_url` / `go_back` | Navigation helpers |
| `save_auth` / `load_auth` / `list_auth` | Auth Vault (domain-scoped) |

**Sequence:** `navigate` → `snapshot` → `click`/`fill` → `snapshot` again after DOM changes.

Full reference: [`MCP.md`](./MCP.md).

### Profile isolation

- MCP `serve` and CLI `run` can run concurrently if each uses its own profile (or rely on auto unique temp profiles in 1.0.0+).
- Pass `--user-data-dir` to MCP **and** CLI when you need a shared persistent session.

---

## 6. Python SDK

```python
from genatp import Agent

agent = Agent(
    provider="nim",
    model="openai/gpt-oss-20b",
    max_tokens=100_000,
    max_wall_seconds=600,
    headless=False,
    user_data_dir=r"C:\Temp\genatp-py-profile",
    record=True,
    record_slug="my-slug",
    ask_user=lambda q, opts: input(f"{q} ") if opts is None else opts[int(input(f"{q} {opts}: "))],
)

result = agent.run(
    "Extract the H1 from example.com",
    start_url="https://example.com",
)
print(result.summary, result.tokens)
```

After Ctrl+C interrupt, resume with same `user_data_dir`:

```python
agent.run("Continue from the current page. Original goal: …", start_url="https://example.com")
```

Async: `await agent.arun(...)`.

Mock smoke (no API key): `GENATP_LLM_PROVIDER=mock`.

---

## 7. Node SDK

```javascript
const { Agent } = require('genatp/agent');  // or package root after publish

const agent = new Agent({
  provider: 'nim',
  model: 'openai/gpt-oss-20b',
  maxTokens: 100_000,
  maxWallSec: 600,
  headless: false,
  userDataDir: 'C:\\Temp\\genatp-node-profile',
  record: true,
  recordSlug: 'my-slug',
});

const result = await agent.run('Extract the H1', {
  startUrl: 'https://example.com',
});
console.log(result.summary, result.tokens);

// After interrupt:
await agent.continueAfterInterrupt('Extract the H1', { startUrl: 'https://example.com' });
```

Build native addon first: `npm run build` in `bindings/node`.

---

## 8. Auth Vault

Concept: seal cookies + `localStorage` per domain under AES-256-GCM; master key in OS keychain.

- **MCP:** `save_auth` after interactive login; `load_auth` on later sessions.
- **CLI:** `genatp vault list` / `revoke`.
- **Scope:** `genatp serve --vault-domains a.com,b.com` limits MCP vault tools.

Design + threat model: [`VAULT.md`](./VAULT.md)  
Live sign-off: [`AUTH_VAULT_LIVE_E2E.md`](./AUTH_VAULT_LIVE_E2E.md)

---

## 9. Action Cassettes

1. **Record:** `genatp record slug url goal` or `genatp run --record --slug slug`
2. **Router hit:** Re-run same goal + start URL → 0 tokens, cassette replay
3. **Manual replay:** `genatp cassette replay slug`
4. **Self-heal:** On ref drift, scoped LLM patch (disable with `--no-heal`)

Cassettes live under `~/.genatp/cassettes/<slug>/versions/<ver>.cassette`.

---

## 10. ATP Spec

Declarative YAML/JSON for tests and automation:

```bash
genatp atp validate my-flow.atp.yml
genatp atp dump my-flow.atp.yml
genatp atp steps my-flow.atp.yml
```

Examples: `examples/atp-spec-ci/`.

---

## 11. Token budgets & reading results

| Line | Meaning |
|---|---|
| `Outcome: success` | Agent finished goal |
| `Outcome: success (cassette replay)` | Router hit — **0 LLM tokens** |
| `Tokens: N` | LLM tokens consumed (0 on replay) |
| `Cassette: path` | Saved recording |

Defaults prevent unbounded NIM burns: **100k tokens**, **600s wall**.

Verified example (NIM `openai/gpt-oss-20b`, example.com H1): ~**1058** tokens first run, **0** on replay.

Peer comparison figures in README are context only until [`BENCHMARK.md`](./BENCHMARK.md) Measured column is filled.

---

## 12. Troubleshooting

| Symptom | Fix |
|---|---|
| Chrome exit **21** | Use `--user-data-dir` isolated path; close conflicting Chrome; check `CHROME_PATH` |
| NIM **410** model EOL | Switch to `openai/gpt-oss-20b` |
| NIM timeout ~300s | Free tier; try shorter goals or different model |
| Stale `genatp` on PATH | Prefer freshly built `target/release` or `target-verify/release` |
| `ask_user` no prompt | Run headed or provide Python `ask_user` callback; needs TTY for stdin |
| MCP + CLI clash | Separate `--user-data-dir` or rely on 1.0.0 unique temp profiles |
| Registry empty | `genatp registry publish <path-to.cassette> <publisher>` |

File shortcomings: [`SHORTCOMINGS_IDENTIFICATION_GUIDE.md`](./SHORTCOMINGS_IDENTIFICATION_GUIDE.md).

---

## 13. Appendix

| Doc | Use |
|---|---|
| [`FULL_SYSTEM_USER_TEST.md`](./FULL_SYSTEM_USER_TEST.md) | End-to-end sign-off matrix |
| [`CLI_NIM_USER_TEST.md`](./CLI_NIM_USER_TEST.md) | NIM-focused CLI checklist |
| [`AUTH_VAULT_MANUAL_TEST.md`](./AUTH_VAULT_MANUAL_TEST.md) | Vault automated + manual gates |
| [`MIGRATION.md`](./MIGRATION.md) | 1.0.0 SDK/CLI flag changes |
| [`BENCHMARK.md`](./BENCHMARK.md) | Measured token methodology |
| [`bindings/python/README.md`](../bindings/python/README.md) | Python API details |
| [`bindings/node/README.md`](../bindings/node/README.md) | Node API details |

### Build from source (contributors)

```bash
make test          # or: cargo test --workspace
make fmt clippy
cargo build --release -p genatp-cli
```

See [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md) for crate map.
