# genATP Python SDK

**`pip install genatp`** — the unified web-access layer for AI agents,
straight into your Python project.

```python
from genatp import Agent

agent = Agent(model="claude-3-5-sonnet-latest")
result = agent.run("Find the cheapest direct flight from SFO to JFK next Friday.")
print(result.summary)
```

The Python wheel ships a native PyO3 extension that wraps the genATP
Rust workspace — the same orchestrator, perception pipeline, and
browser substrate the MCP server uses. Identical semantics, three
languages.

| Surface                                    | Module                          |
| ------------------------------------------ | ------------------------------- |
| Top-level Agent facade                     | [`genatp.Agent`](api/agent.md)  |
| Browser session (raw MCP tools)            | [`genatp.BrowserSession`](api/browser.md) |
| LangChain adapter                          | [`genatp.langchain`](api/langchain.md) |
| LlamaIndex adapter                         | [`genatp.llamaindex`](api/llamaindex.md) |
| Tool catalogue                             | [`genatp.tools`](api/tools.md)  |
| Provider discovery                         | [`genatp.providers`](api/providers.md) |
| Error hierarchy                            | [`genatp.errors`](api/errors.md) |

## Where to start

- **[Quickstart](quickstart.md)** — install, configure a provider, run
  your first agent in 60 seconds.
- **[Agent API](api/agent.md)** — the high-level `Agent` class.
- **[LangChain adapter](api/langchain.md)** — wire the nine MCP tools
  into a LangGraph ReAct agent.

## Compatibility matrix

| OS         | Arch    | Python    | Wheel       |
| ---------- | ------- | --------- | ----------- |
| Linux      | x86_64  | 3.10–3.13 | abi3 wheel  |
| Linux      | aarch64 | 3.10–3.13 | abi3 wheel  |
| macOS      | x86_64  | 3.10–3.13 | abi3 wheel  |
| macOS      | arm64   | 3.10–3.13 | abi3 wheel  |
| Windows    | x86_64  | 3.10–3.13 | abi3 wheel  |

All five wheels ship from a single CI build (matrix in
`.github/workflows/python-wheels.yml`).

## License

Source-available under the **genATP Personal Use License**. Commercial
use requires a paid license — see
[the repo LICENSE](https://github.com/genatp/genatp/blob/main/LICENSE).
