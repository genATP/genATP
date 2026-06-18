import type { SaveAuthSummary, LoadAuthSummary, AuthDomain } from './types';

export interface BrowserSessionOptions {
  headless?: boolean;
  userDataDir?: string;
}

/**
 * A direct browser session. Owns a Chrome subprocess.
 * Always call close() when done.
 */
export class BrowserSession {
  constructor(options?: BrowserSessionOptions);

  /** Navigate to a URL. Returns the final URL after redirects. */
  navigate(url: string): Promise<string>;

  /** Capture a fresh perception snapshot. Returns JSON string. */
  snapshot(): Promise<string>;

  /** Click an element identified by @eN ref. */
  click(refstr: string): Promise<void>;

  /** Fill a text input identified by @eN ref. */
  fill(refstr: string, value: string): Promise<void>;

  /** Scroll the viewport by relative pixels. */
  scroll(dx: number, dy: number): Promise<void>;

  /** Return the current page URL. */
  currentUrl(): Promise<string>;

  /** Navigate back in history. Returns true if navigation happened. */
  goBack(): Promise<boolean>;

  /** Take a full-viewport PNG screenshot. Returns base64-encoded string. */
  screenshot(): Promise<string>;

  /** Extract visible text from the latest snapshot. */
  extract(query?: string): Promise<string>;

  /** Save auth state (cookies + localStorage) for a domain to the vault. */
  saveAuth(domain: string): Promise<SaveAuthSummary>;

  /** Load auth state for a domain from the vault into the current page. */
  loadAuth(domain: string): Promise<LoadAuthSummary>;

  /** List all domains stored in the auth vault. */
  listAuth(): Promise<AuthDomain[]>;

  /** Close the session. Idempotent. */
  close(): Promise<void>;
}
