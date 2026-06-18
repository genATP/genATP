//! genATP TypeScript SDK — PRD 5-line quickstart.
//!
//! Run with:
//!   node examples/quickstart.mjs
//!
//! Requires:
//! - The native addon to be built (`npm run build` or `napi build --platform --release`)
//! - An LLM API key in the environment (or Ollama running locally)

import { Agent } from '../index.js';

async function main() {
  const agent = new Agent({ model: 'claude-3-5-sonnet-latest' });
  const result = await agent.run(
    'Find the cheapest direct flight from SFO to JFK next Friday.',
    { startUrl: 'https://www.google.com/travel/flights' },
  );
  console.log(result.summary);
  console.log(`Steps: ${result.steps}, Tokens: ${result.spend.tokens}`);
}

main().catch(console.error);
