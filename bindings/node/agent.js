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
 * Each run() launches a browser, runs the plan loop, then tears Chrome down.
 * Pass a stable userDataDir (and Auth Vault) to keep sessions across runs.
 * Ctrl+C cooperatively aborts after the current step.
 */
class Agent {
  /**
   * @param {object} options
   * @param {string} [options.model] - LLM model id.
   * @param {'anthropic'|'openai'|'nim'|'ollama'|'mock'} [options.provider] - Provider kind.
   * @param {number} [options.maxSteps=30] - Step ceiling.
   * @param {number} [options.maxTokens=100000] - Hard token budget (0 = unlimited).
   * @param {number} [options.maxWallSec=600] - Wall-clock budget (0 = unlimited).
   * @param {number} [options.maxUsd=0] - Dollar budget (0 = unlimited).
   * @param {boolean} [options.headless=true] - Chrome headless mode.
   * @param {string} [options.userDataDir] - Persistent profile path.
   * @param {string} [options.systemPrompt] - Override the default system prompt.
   * @param {string} [options.mockReply] - Mock provider response.
   * @param {boolean} [options.record=false] - Save Action Cassette on success.
   * @param {string} [options.recordSlug] - Cassette slug when recording.
   */
  constructor(options = {}) {
    this.model = options.model || null;
    this.provider = options.provider || null;
    this.maxSteps = options.maxSteps ?? 30;
    this.maxTokens = options.maxTokens ?? 100_000;
    this.maxWallSec = options.maxWallSec ?? 600;
    this.maxUsd = options.maxUsd ?? 0;
    this.headless = options.headless ?? true;
    this.userDataDir = options.userDataDir || null;
    this.systemPrompt = options.systemPrompt || null;
    this.mockReply = options.mockReply || null;
    this.record = options.record ?? false;
    this.recordSlug = options.recordSlug || null;
  }

  /**
   * Run the agent against one goal.
   *
   * @param {string} goal - Plain-English task description.
   * @param {object} [options] - Optional overrides.
   * @param {string} [options.startUrl] - URL the agent navigates to first.
   * @param {boolean} [options.record] - Override cassette recording for this run.
   * @param {string} [options.recordSlug] - Override cassette slug for this run.
   * @returns {Promise<RunResult>}
   */
  async run(goal, options = {}) {
    const startUrl = options.startUrl || DEFAULT_START_URL;
    const doRecord = options.record === undefined ? this.record : options.record;
    const slug = options.recordSlug === undefined ? this.recordSlug : options.recordSlug;
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
        doRecord,
        slug,
      );
      if (nativeResult instanceof Error) throw nativeResult;
      return new RunResult(nativeResult);
    } catch (err) {
      throw new Error(`genATP agent run failed: ${err.message}`);
    }
  }
  /**
   * Resume after a cooperative interrupt by restudying the page.
   * Requires a persistent userDataDir so the profile still exists.
   *
   * @param {string} originalGoal
   * @param {object} [options]
   * @param {string} [options.startUrl]
   * @returns {Promise<RunResult>}
   */
  async continueAfterInterrupt(originalGoal, options = {}) {
    if (!this.userDataDir) {
      throw new Error(
        'continueAfterInterrupt requires userDataDir so the browser profile persists',
      );
    }
    const goal = `Continue from the current page state. Original goal: ${originalGoal}`;
    return this.run(goal, options);
  }
}

module.exports = { Agent };
