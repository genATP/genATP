//! Error type surfaced by `genatp-browser`.
//!
//! Wraps the noisy `chromiumoxide::error::CdpError` enum into stable variants
//! so downstream crates (perception, orchestrator) match on intent, not on
//! upstream library churn.

use std::time::Duration;

/// Errors returned by the browser substrate.
#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    /// Failed to launch Chrome (binary missing, port collision, sandbox issue).
    #[error("failed to launch browser: {0}")]
    Launch(String),

    /// CDP transport returned an error or the connection dropped mid-command.
    #[error("CDP transport error: {0}")]
    Cdp(String),

    /// `Page.navigate` either timed out, returned a non-2xx status, or aborted.
    #[error("navigation to {url} failed: {reason}")]
    Navigate {
        /// Target URL the orchestrator tried to load.
        url: String,
        /// Human-readable reason from Chrome (e.g. `ERR_CONNECTION_REFUSED`).
        reason: String,
    },

    /// Operation exceeded its budget; surfaced before resources are leaked.
    #[error("operation timed out after {0:?}")]
    Timeout(Duration),

    /// Pool is at capacity and the caller did not opt into waiting.
    #[error("browser pool exhausted ({active}/{max} contexts in use)")]
    PoolExhausted {
        /// Number of contexts currently in use.
        active: usize,
        /// Configured max concurrency.
        max: usize,
    },

    /// Caller-supplied URL was malformed.
    #[error("invalid URL {url:?}: {source}")]
    InvalidUrl {
        /// The string that failed to parse.
        url: String,
        /// Parser diagnostic from `url::Url::parse`.
        #[source]
        source: url::ParseError,
    },

    /// Catch-all for I/O surfaced from the browser handler.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl BrowserError {
    /// True if the error is plausibly recoverable by retrying the same operation.
    /// Used by the orchestrator's retry policy in Phase 3.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Cdp(_) | Self::Timeout(_) | Self::Navigate { .. } | Self::Io(_)
        )
    }
}

impl From<chromiumoxide::error::CdpError> for BrowserError {
    fn from(err: chromiumoxide::error::CdpError) -> Self {
        Self::Cdp(err.to_string())
    }
}

/// Convenience alias used throughout the crate.
pub type Result<T, E = BrowserError> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_is_retryable() {
        assert!(BrowserError::Timeout(Duration::from_secs(1)).is_retryable());
    }

    #[test]
    fn pool_exhausted_is_not_retryable() {
        let err = BrowserError::PoolExhausted { active: 4, max: 4 };
        assert!(!err.is_retryable());
    }

    #[test]
    fn invalid_url_is_not_retryable() {
        let err = BrowserError::InvalidUrl {
            url: "not a url".into(),
            source: url::Url::parse("not a url").unwrap_err(),
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn display_includes_url_for_navigate() {
        let err = BrowserError::Navigate {
            url: "https://example.com".into(),
            reason: "ECONNREFUSED".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("https://example.com"));
        assert!(msg.contains("ECONNREFUSED"));
    }
}
