import type { RunResult } from './types';

export interface AgentOptions {
  model?: string;
  provider?: 'anthropic' | 'openai' | 'nim' | 'ollama' | 'mock';
  maxSteps?: number;
  /** Hard token budget. Default 100000; pass 0 for unlimited. */
  maxTokens?: number;
  /** Wall-clock seconds. Default 600; pass 0 for unlimited. */
  maxWallSec?: number;
  maxUsd?: number;
  headless?: boolean;
  userDataDir?: string;
  systemPrompt?: string;
  mockReply?: string;
  /** Save an Action Cassette on successful runs. */
  record?: boolean;
  recordSlug?: string;
}

export interface RunOptions {
  startUrl?: string;
  record?: boolean;
  recordSlug?: string;
}

/**
 * A single agent able to drive a real browser to accomplish tasks.
 *
 * Each run() launches a browser, runs the plan loop, then tears Chrome down.
 * Pass a stable userDataDir to keep sessions; Ctrl+C aborts after the current step.
 * Headed runs (`headless: false`) prompt on stdin for ask_user.
 */
export class Agent {
  constructor(options?: AgentOptions);
  run(goal: string, options?: RunOptions): Promise<RunResult>;
  /** Resume after Ctrl+C; requires userDataDir. */
  continueAfterInterrupt(originalGoal: string, options?: RunOptions): Promise<RunResult>;
}
