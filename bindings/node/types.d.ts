/** Token + USD tally for a single agent run. */
export class BudgetSpend {
  /** Total prompt + completion tokens charged. */
  readonly tokens: number;
  /** Cumulative USD spend (0 for free providers). */
  readonly usd: number;
  /** Number of events recorded in the hash-chained event log. */
  readonly eventCount: number;
}

/** Outcome of one Agent.run() invocation. */
export class RunResult {
  readonly outcome: 'success' | 'aborted' | 'error';
  readonly summary: string;
  readonly steps: number;
  readonly spend: BudgetSpend;

  /** True iff the agent finished successfully. */
  get ok(): boolean;
}

/** Snapshot of a resolved LLM provider. */
export class ProviderInfo {
  readonly kind: string;
  readonly name: string;
  readonly model: string;
}

/** One MCP tool descriptor. */
export class ToolDescriptor {
  readonly name: string;
  readonly description: string;
  readonly inputSchema: Record<string, unknown>;
}

/** Summary of a save_auth operation. */
export class SaveAuthSummary {
  readonly cookies: number;
  readonly localStorage: number;
  readonly sessionStorage: number;
  readonly cookieNames: string[];
}

/** Summary of a load_auth operation. */
export class LoadAuthSummary {
  readonly cookies: number;
  readonly localStorage: number;
  readonly sessionStorage: number;
}

/** One auth domain entry. */
export class AuthDomain {
  readonly domain: string;
  readonly entryCount: number;
}

/** Default start URL for the agent. */
export const DEFAULT_START_URL: string;
