# genATP — TypeScript/Node SDK

**The unified web-access layer for AI agents — `npm install genatp`.**

```bash
npm install genatp
```

```typescript
import { Agent } from 'genatp';

const agent = new Agent({ model: 'claude-3-5-sonnet-latest' });
const result = await agent.run(
  'Find the cheapest direct flight from SFO to JFK next Friday.',
);
console.log(result.summary);
```

That's the entire PRD quickstart. The Rust orchestrator drives a real Chrome
instance via CDP, perception compresses the accessibility tree, the LLM picks
the next action, and the loop terminates when the agent calls `finish`.

## What's in the box

| Surface                 | Export                          |
| ----------------------- | ------------------------------- |
| Top-level Agent         | `Agent`                         |
| Browser session (MCP)   | `BrowserSession`                |
| Tool descriptors        | `listTools()`                   |
| Diagnostics             | `buildInfo()`                   |
| Provider resolution     | `resolveProviderFromEnv()`      |

## Usage

### Agent (high-level)

```typescript
import { Agent } from 'genatp';

const agent = new Agent({ model: 'gpt-4o-mini' });
const result = await agent.run('Summarize the top story on news.ycombinator.com');
console.log(result.summary, result.spend.tokens);
```

### BrowserSession (low-level)

```typescript
import { BrowserSession } from 'genatp';

const session = new BrowserSession({ headless: true });
const url = await session.navigate('https://example.com');
const snapshot = await session.snapshot();
console.log(snapshot);
await session.close();
```

## Provider selection

genATP picks a provider automatically from env vars:

| Env var             | Provider                |
| ------------------- | ----------------------- |
| `ANTHROPIC_API_KEY` | Claude                  |
| `NVIDIA_API_KEY`    | NVIDIA NIM (free tier)  |
| `OPENAI_API_KEY`    | OpenAI                  |
| _(none)_            | Ollama at localhost     |

Override with `new Agent({ provider: 'anthropic' })`.

## Building from source

```bash
cd bindings/node
npm install
npm run build
npm test
```

## License

`genATP` is source-available under the **genATP Personal Use License**.
Any commercial use requires a paid license; see the repo `LICENSE`.
