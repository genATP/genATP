//! BrowserSession — direct browser control via the MCP tool surface.

const native = require('./js/index.js');
const { SaveAuthSummary, LoadAuthSummary, AuthDomain } = require('./types.js');

/**
 * A direct browser session. Owns a Chrome subprocess.
 * Always call close() when done.
 */
class BrowserSession {
  /**
   * @param {object} [options]
   * @param {boolean} [options.headless=true] - Launch Chrome in headless mode.
   * @param {string} [options.userDataDir] - Persistent profile directory.
   */
  constructor(options = {}) {
    this._session = new native.BrowserSession(
      options.headless ?? true,
      options.userDataDir || null,
    );
  }

  /** Navigate to a URL. Returns the final URL after redirects. */
  async navigate(url) {
    return this._session.navigate(url);
  }

  /** Capture a fresh perception snapshot. Returns the JSON string. */
  async snapshot() {
    return this._session.snapshot();
  }

  /** Click an element identified by @eN ref. */
  async click(refstr) {
    return this._session.click(refstr);
  }

  /** Fill a text input identified by @eN ref. */
  async fill(refstr, value) {
    return this._session.fill(refstr, value);
  }

  /** Scroll the viewport by relative pixels. */
  async scroll(dx, dy) {
    return this._session.scroll(dx, dy);
  }

  /** Return the current page URL. */
  async currentUrl() {
    return this._session.currentUrl();
  }

  /** Navigate back in history. Returns true if it happened. */
  async goBack() {
    return this._session.goBack();
  }

  /** Take a full-viewport PNG screenshot. Returns base64 string. */
  async screenshot() {
    return this._session.screenshot();
  }

  /** Extract visible text from latest snapshot, optionally filtered. */
  async extract(query) {
    return this._session.extract(query || null);
  }

  /** Save auth state for a domain to the vault. */
  async saveAuth(domain) {
    return new SaveAuthSummary(await this._session.saveAuth(domain));
  }

  /** Load auth state for a domain from the vault. */
  async loadAuth(domain) {
    return new LoadAuthSummary(await this._session.loadAuth(domain));
  }

  /** List all domains stored in the auth vault. */
  async listAuth() {
    const raw = await this._session.listAuth();
    return raw.map((d) => new AuthDomain(d.domain, d.entryCount));
  }

  /** Close the session. Idempotent. */
  async close() {
    this._session.close();
  }
}

module.exports = { BrowserSession };
