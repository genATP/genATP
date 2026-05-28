# genATP MCP server

genATP v0.1.0 ships an MCP (Model Context Protocol) server that exposes
browser control to any MCP-compatible LLM client — Claude Desktop, Cursor,
the MCP Inspector, the TypeScript / Python MCP SDKs, etc.

This document covers:

- [Quickstart](#quickstart) (build + install)
- [Claude Desktop setup](#claude-desktop)
- [Cursor setup](#cursor-setup)
- [MCP Inspector](#mcp-inspector)
- [Tool reference](#tools)
- [Resource reference](#resources)
- [Transports](#transports)
- [Logging & troubleshooting](#troubleshooting)
- [Security notes](#security-notes)

---

## Quickstart

Build the binary:

```bash
cargo build --release -p genatp-cli
```

Install on `PATH` (optional but recommended):

```bash
cargo install --path crates/genatp-cli
# binary lands at: ~/.cargo/bin/genatp
```

Smoke test:

```bash
genatp version
# genatp 0.1.0

genatp serve --stdio
# Waits on stdin for JSON-RPC messages. Ctrl-C to exit.
```

---

## Claude Desktop

1. Locate Claude Desktop's config file:

   | OS      | Path                                                             |
   | ------- | ---------------------------------------------------------------- |
   | macOS   | `~/Library/Application Support/Claude/claude_desktop_config.json` |
   | Linux   | `~/.config/Claude/claude_desktop_config.json`                    |
   | Windows | `%APPDATA%\Claude\claude_desktop_config.json`                    |

2. Add genATP under `mcpServers`:

   ```json
   {
     "mcpServers": {
       "genatp": {
         "command": "genatp",
         "args": ["serve", "--stdio"],
         "env": { "RUST_LOG": "warn" }
       }
     }
   }
   ```

   See [`examples/mcp-claude-desktop/mcp_config.json`](../examples/mcp-claude-desktop/mcp_config.json)
   for a copy-pasteable version.

3. Restart Claude Desktop. The new MCP server appears in the connection
   indicator. Tell Claude something like *"open github.com and tell me
   what's on the homepage"* — it'll pick the genATP tools.

If `genatp` is not on your `PATH`, use the absolute path in `command`.

---

## Cursor setup

Cursor uses the same MCP wire format. Add the server under
**Settings → Cursor MCP**:

```json
{
  "mcpServers": {
    "genatp": {
      "command": "/absolute/path/to/genatp",
      "args": ["serve", "--stdio"]
    }
  }
}
```

Restart Cursor. The tools become available in the agent's tool palette.

---

## MCP Inspector

For interactive exploration, use the
[MCP Inspector](https://github.com/modelcontextprotocol/inspector):

```bash
npx @modelcontextprotocol/inspector genatp serve --stdio
```

The inspector launches a UI that walks through `initialize`,
`tools/list`, and lets you invoke each tool with arbitrary arguments.

---

## Tools

All nine tools (v0.1.0):

| Name          | Arguments                                            | Result                                           |
| ------------- | ---------------------------------------------------- | ------------------------------------------------ |
| `navigate`    | `url: string`                                        | text — final URL after redirects                 |
| `snapshot`    | _(none)_                                             | text — JSON CompressedSnapshot                   |
| `click`       | `ref: "@eN"`                                         | text — confirmation                              |
| `fill`        | `ref: "@eN"`, `value: string`                        | text — confirmation                              |
| `extract`     | `query?: string` (case-insensitive substring filter) | text — visible content                           |
| `scroll`      | `dx?: int`, `dy?: int`                               | text — confirmation                              |
| `screenshot`  | _(none)_                                             | image — base64 PNG                               |
| `get_url`     | _(none)_                                             | text — current URL                               |
| `go_back`     | _(none)_                                             | text — `"navigated back"` or `"no prior page"`   |

### Idiomatic call sequence

The agent **must** call `snapshot` before any element-targeting tool:

```text
navigate(https://example.com)
snapshot()                       → returns @e1 … @eN refs
click(ref="@e7")                 → uses a ref from the snapshot
snapshot()                       → re-snapshot after state changes
fill(ref="@e3", value="hello")
```

`@eN` refs are **stable across snapshots** for the same logical element
(see `docs/PERCEPTION.md`). If a ref no longer resolves (because the
page navigated away), `click`/`fill` returns `isError: true` with a
message asking the agent to re-snapshot.

---

## Resources

| URI                          | MIME              | Content                                 |
| ---------------------------- | ----------------- | --------------------------------------- |
| `genatp://snapshot/current`  | `application/json` | Most-recent `snapshot` JSON; `{}` if none yet |

Fetched via `resources/read` per the MCP spec.

---

## Transports

### stdio (primary)

```bash
genatp serve --stdio        # default; the only transport Claude Desktop uses
```

Newline-delimited JSON-RPC 2.0 on stdin/stdout. **stdout is reserved**
for protocol traffic — all logs go to stderr.

### HTTP (secondary)

```bash
genatp serve --port 3000    # binds 127.0.0.1:3000
```

| Endpoint    | Method | Body                              |
| ----------- | ------ | --------------------------------- |
| `/`         | `POST` | One JSON-RPC message in, one out  |
| `/healthz`  | `GET`  | `200 OK` if server is healthy     |

Useful for browser-side MCP clients, integration tests, and curl-style
debugging:

```bash
curl -sX POST http://127.0.0.1:3000/ \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize",
       "params":{"protocolVersion":"2024-11-05",
                 "capabilities":{},
                 "clientInfo":{"name":"curl","version":"1"}}}'
```

> The HTTP transport binds to **loopback only**. Do not expose it to
> the public internet — there is no auth and tools can drive a real
> browser.

---

## Troubleshooting

### "Server failed to start" in Claude Desktop

Run from a terminal first to see stderr:

```bash
RUST_LOG=info genatp serve --stdio < /dev/null
```

If Chrome isn't found, set `CHROME_PATH`:

```bash
export CHROME_PATH="/path/to/chrome"
```

### Tools return `"ref @eN not in current snapshot"`

The agent forgot to call `snapshot` after a navigation. Re-snapshot
before issuing the next `click` / `fill`. genATP's tool descriptions
hint at this; teach your prompt to follow the canonical sequence.

### Verbose logs

```bash
RUST_LOG=debug genatp -vv serve --stdio
```

`-v` raises the default filter to `info`, `-vv` to `debug`. Per-module
filters via `RUST_LOG` override the level (e.g.
`RUST_LOG=genatp_browser=trace,info`).

### Headed (visible) Chrome for debugging

```bash
genatp serve --stdio --headed
```

The browser window appears so you can watch the agent click in real
time.

### Persistent cookies / logins

```bash
genatp serve --stdio --user-data-dir ~/.genatp-profile
```

Reuses Chrome's profile across runs. Auth Vault (Phase 6) will replace
this with capability-gated, per-domain credential sealing.

---

## Security notes

- **Tools drive a real browser.** Whoever can speak JSON-RPC to the
  server can navigate, click, and read pages. The stdio transport
  inherits the security domain of the parent process; the HTTP
  transport is bound to `127.0.0.1` only.
- **No secrets are logged.** Tool arguments (including filled values)
  flow through `tracing` only at `debug`+ levels.
- **Cross-origin requests** from the LLM are unrestricted by default
  — if the LLM is hostile, *anything* it can reach in your browser
  profile is reachable. Use a fresh `--user-data-dir` for untrusted
  agents.
- **API keys**: the MCP server itself does **not** read any provider
  API key — it only drives Chrome, so LLM API keys live in the *client*
  (Claude Desktop, Cursor, etc.). genATP's separate LLM client
  (`genatp-llm`) reads keys from `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`,
  `NVIDIA_API_KEY`, or the local Ollama daemon — see
  [`CONFIGURATION.md`](./CONFIGURATION.md) for the full list — and
  never logs them.

---

## Spec compliance

- MCP protocol version: `2024-11-05`
- JSON-RPC version: `2.0`
- Standard error codes per [JSON-RPC 2.0 §5.1](https://www.jsonrpc.org/specification#error_object)
- Tool schemas conform to JSON-Schema draft 2020-12

For details on the JSON-RPC envelope shapes, see
[`crates/genatp-mcp/src/protocol.rs`](../crates/genatp-mcp/src/protocol.rs).
