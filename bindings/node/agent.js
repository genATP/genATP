//! High-level Agent facade — the public face of the Node SDK.
//!
//! PRD 5-line quickstart:
//!
//! ```typescript
//! import { Agent } from 'genatp';
//!
//! const agent = new Agent({ model: 'claude-3-5-sonnet-latest' });
//! const result = await agent.run('Find the cheapest direct flight from SFO to JFK next Friday.');
//! console.log(result.summary);
//! ```

const native = require('./js/index.js');
const { RunResult, DEFAULT_START_URL } = require('./types.js');

/**
 * A single agent able to drive a real browser to accomplish tasks.
 *
 * The agent is stateless across calls: each run() launches a fresh
 * browser, runs the plan loop, then tears Chrome down.
 */
class Agent {
  /**
   * @param {object} options
   * @param {string} [options.model] - LLM model id.
   * @param {'anthropic'|'openai'|'nim'|'ollama'|'mock'} [options.provider] - Provider kind.
   * @param {number} [options.maxSteps=30] - Step ceiling.
   * @param {number} [options.maxTokens=0] - Hard token budget (0 = unlimited).
   * @param {number} [options.maxWallSec=0] - Wall-clock budget (0 = unlimited).
   * @param {number} [options.maxUsd=0] - Dollar budget (0 = unlimited).
   * @param {boolean} [options.headless=true] - Chrome headless mode.
   * @param {string} [options.userDataDir] - Persistent profile path.
   * @param {string} [options.systemPrompt] - Override the default system prompt.
   * @param {string} [options.mockReply] - Mock provider response.
   */
  constructor(options = {}) {
    this.model = options.model || null;
    this.provider = options.provider || null;
    this.maxSteps = options.maxSteps ?? 30;
    this.maxTokens = options.maxTokens ?? 0;
    this.maxWallSec = options.maxWallSec ?? 0;
    this.maxUsd = options.maxUsd ?? 0;
    this.headless = options.headless ?? true;
    this.userDataDir = options.userDataDir || null;
    this.systemPrompt = options.systemPrompt || null;
    this.mockReply = options.mockReply || null;
  }

  /**
   * Run the agent against one goal.
   *
   * @param {string} goal - Plain-English task description.
   * @param {object} [options] - Optional overrides.
   * @param {string} [options.startUrl] - URL the agent navigates to first.
   * @returns {Promise<RunResult>}
   */
  async run(goal, options = {}) {
    const startUrl = options.startUrl || DEFAULT_START_URL;
    try {
      const nativeResult = native.runAgent(
        goal,
        startUrl,
        this.model,
        this.maxSteps,
        this.maxTokens,
        this.maxWallSec,
        this.maxUsd,
        this.systemPrompt,
        this.headless,
        this.userDataDir,
        this.provider,
        this.mockReply,
      );
      if (nativeResult instanceof Error) throw nativeResult;
      return new RunResult(nativeResult);
    } catch (err) {
      throw new Error(`genATP agent run failed: ${err.message}`);
    }
  }
}

module.exports = { Agent };
