# genATP — Python SDK

**The unified web-access layer for AI agents — `pip install genatp`.**

```bash
pip install genatp
```

```python
from genatp import Agent

agent = Agent(model="claude-3-5-sonnet-latest")
result = agent.run(
    "Find the cheapest direct flight from SFO to JFK next Friday.",
    start_url="https://www.google.com/travel/flights",
)
print(result.summary)
```

That's the entire PRD quickstart. The Rust orchestrator drives a real Chrome
instance via CDP, perception compresses the accessibility tree, the LLM picks
the next action, and the loop terminates when the agent calls `finish`.

## What's in the box

| Surface                     | Module                              |
| --------------------------- | ----------------------------------- |
| Top-level Agent             | `genatp.Agent`                      |
| Async API                   | `genatp.Agent.arun`                 |
| Browser session (MCP-level) | `genatp.BrowserSession`             |
| LangChain adapter           | `genatp.langchain.as_langchain_tools` |
| LlamaIndex adapter          | `genatp.llamaindex.as_llamaindex_tools` |
| Tool descriptors            | `genatp.tools.list_tools`           |
| Diagnostics                 | `genatp.build_info()`               |

## Async usage

```python
import asyncio
from genatp import Agent

async def main():
    agent = Agent(model="gpt-4o-mini")
    result = await agent.arun("Summarize the top story on news.ycombinator.com")
    print(result.summary)

asyncio.run(main())
```

## LangChain integration

```python
from genatp.langchain import as_langchain_tools
from langgraph.prebuilt import create_react_agent

tools = as_langchain_tools()           # 9 production-ready tools
react = create_react_agent(model, tools=tools)
```

## Provider selection

genATP picks a provider automatically from env vars (see
[`docs/CONFIGURATION.md`](../../docs/CONFIGURATION.md)):

| Env var            | Provider                |
| ------------------ | ----------------------- |
| `ANTHROPIC_API_KEY`| Claude                  |
| `NVIDIA_API_KEY`   | NVIDIA NIM (free tier)  |
| `OPENAI_API_KEY`   | OpenAI                  |
| _(none)_           | Ollama at `127.0.0.1:11434` |

Override with `Agent(provider="anthropic")` or by setting
`GENATP_LLM_PROVIDER`.

## License

`genATP` is source-available under the **genATP Personal Use License**.
Any commercial use requires a paid license; see the repo `LICENSE` and
`licensing@genatp.dev`.
