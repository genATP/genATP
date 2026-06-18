/// <reference types="./js/index" />

import type {
  RunResult,
  BudgetSpend,
  ProviderInfo,
  ToolDescriptor,
  SaveAuthSummary,
  LoadAuthSummary,
  AuthDomain,
} from './types';

export { Agent } from './agent';
export { BrowserSession } from './browser';
export type {
  RunResult,
  BudgetSpend,
  ProviderInfo,
  ToolDescriptor,
  SaveAuthSummary,
  LoadAuthSummary,
  AuthDomain,
} from './types';

/** Return the native addon version string. */
export function version(): string;

/** Resolve an LLM provider from environment variables. */
export function resolveProviderFromEnv(): ProviderInfo;

/** Return the canonical list of MCP tool descriptors. */
export function listTools(): ToolDescriptor[];

/** Return build info metadata. */
export function buildInfo(): Record<string, unknown>;

/** Default start URL for the agent. */
export const DEFAULT_START_URL: string;
