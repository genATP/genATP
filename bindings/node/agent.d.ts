import type { RunResult } from './types';

export interface AgentOptions {
  model?: string;
  provider?: 'anthropic' | 'openai' | 'nim' | 'ollama' | 'mock';
  maxSteps?: number;
  maxTokens?: number;
  maxWallSec?: number;
  maxUsd?: number;
  headless?: boolean;
  userDataDir?: string;
  systemPrompt?: string;
  mockReply?: string;
}

export interface RunOptions {
  startUrl?: string;
}

/**
 * A single agent able to drive a real browser to accomplish tasks.
 *
 * The agent is stateless across calls: each run() launches a fresh
 * browser, runs the plan loop, then tears Chrome down.
 */
export class Agent {
  constructor(options?: AgentOptions);
  run(goal: string, options?: RunOptions): Promise<RunResult>;
}
