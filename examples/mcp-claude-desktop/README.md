# Claude Desktop config snippet

Drop the contents of [`mcp_config.json`](./mcp_config.json) into Claude Desktop's
configuration file:

- **macOS / Linux** — `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows** — `%APPDATA%\Claude\claude_desktop_config.json`

If the file already contains an `mcpServers` block, merge our entry into it.
Restart Claude Desktop after editing.

If `genatp` is not on your `PATH`, replace `"command": "genatp"` with the
absolute path to the binary (`cargo install --path crates/genatp-cli` puts it
at `~/.cargo/bin/genatp` by default).

See [`docs/MCP.md`](../../docs/MCP.md) for the full setup walk-through.
