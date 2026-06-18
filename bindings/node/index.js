//! genATP — Node.js SDK
//!
//! The public API for the genATP TypeScript/Node SDK.
//!
//! ## PRD 5-line quickstart
//!
//! ```typescript
//! import { Agent } from 'genatp';
//!
//! const agent = new Agent({ model: 'claude-3-5-sonnet-latest' });
//! const result = await agent.run('Find the cheapest direct flight from SFO to JFK next Friday.');
//! console.log(result.summary);
//! ```

// Load the napi-rs generated bindings.
const native = require('./js/index.js');

const { Agent } = require('./agent.js');
const { BrowserSession } = require('./browser.js');
const {
  RunResult,
  BudgetSpend,
  ProviderInfo,
  ToolDescriptor,
  SaveAuthSummary,
  LoadAuthSummary,
  AuthDomain,
  DEFAULT_START_URL,
} = require('./types.js');

/** Return the native addon version string. */
function version() {
  return native.version();
}

/** Resolve an LLM provider from environment variables. */
function resolveProviderFromEnv() {
  return native.resolveProviderFromEnv();
}

/** Return the canonical list of MCP tool descriptors. */
function listTools() {
  return native.listMcpTools();
}

/** Return build info metadata. */
function buildInfo() {
  return native.buildInfo();
}

module.exports = {
  Agent,
  BrowserSession,
  RunResult,
  BudgetSpend,
  ProviderInfo,
  ToolDescriptor,
  SaveAuthSummary,
  LoadAuthSummary,
  AuthDomain,
  version,
  resolveProviderFromEnv,
  listTools,
  buildInfo,
  DEFAULT_START_URL,
};
