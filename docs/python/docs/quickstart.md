# Quickstart

## 1. Install

```bash
pip install genatp
```

That single command pulls down a pre-built wheel for your platform.
No Rust toolchain needed at install time — the abi3 wheel works on
Python 3.10+ unchanged.

## 2. Configure a provider

genATP picks an LLM provider automatically from environment variables:

| Set this                | …and you get             |
| ----------------------- | ------------------------ |
| `ANTHROPIC_API_KEY=...` | Claude                   |
| `NVIDIA_API_KEY=...`    | NVIDIA NIM (free tier)   |
| `OPENAI_API_KEY=...`    | OpenAI                   |
| _(nothing)_             | Ollama at `127.0.0.1:11434` |

You can override explicitly:

```python
agent = Agent(provider="nim", model="meta/llama-4-maverick-17b-128e-instruct")
```

See [`docs/CONFIGURATION.md`](https://github.com/genatp/genatp/blob/main/docs/CONFIGURATION.md)
in the repo for the exhaustive list.

## 3. Run an agent

```python
from genatp import Agent

agent = Agent(model="claude-3-5-sonnet-latest", max_wall_seconds=120)
result = agent.run(
    "Open example.com and tell me the page title.",
    start_url="https://example.com",
)
print(result.outcome, result.summary)
```

## 4. Async usage

```python
import asyncio
from genatp import Agent

async def main():
    agent = Agent(model="gpt-4o-mini")
    result = await agent.arun("Find today's top story on news.ycombinator.com.")
    print(result.summary)

asyncio.run(main())
```

`Agent.arun` calls `asyncio.to_thread` under the hood — the calling
event loop stays unblocked while Chrome and the LLM finish their
roundtrips.

## 5. Verify your install

```bash
python -c "import genatp; print(genatp.build_info())"
```

You should see something like:

```
{'genatp_python': '0.2.0', 'genatp_core': '0.2.0', ..., 'mcp_tools': ['navigate', ...]}
```

## 6. Smoke test without an LLM

genATP ships a `mock` provider that's perfect for CI smoke tests — no
API key, no Ollama, no network:

```python
agent = Agent(provider="mock", mock_reply="all done")
result = agent.run(
    "Greet me and finish.",
    start_url="data:text/html,<h1>hi</h1>",
)
assert result.spend.event_count > 0
```
