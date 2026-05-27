//! Error types for [`crate::LlmProvider`] implementations.
//!
//! Every adapter funnels its failures through [`LlmError`] so the
//! orchestrator (Phase 3) can make uniform retry / give-up decisions
//! without parsing provider-specific strings.

use std::time::Duration;

/// Crate-wide alias.
pub type Result<T> = std::result::Result<T, LlmError>;

/// All errors a provider can surface.
///
/// Variants are ordered loosely from "transient" (worth retrying) at the
/// top to "permanent" at the bottom. The [`LlmError::is_retryable`] helper
/// codifies that ordering.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// Underlying transport error (DNS, TCP, TLS, connection reset). Almost
    /// always retryable.
    #[error("transport: {0}")]
    Transport(String),

    /// Provider returned HTTP 429 / RPC equivalent — back off.
    #[error("rate limit (retry after {retry_after:?})")]
    RateLimited { retry_after: Option<Duration> },

    /// Provider returned HTTP 5xx — typically retryable.
    #[error("server error (status {status}): {message}")]
    ServerError { status: u16, message: String },

    /// We hit a per-request budget (token, time) the caller declared. Not
    /// retryable without changing the budget.
    #[error("budget exceeded: {0}")]
    BudgetExceeded(String),

    /// Wall-clock timeout expired before the provider answered.
    #[error("timeout after {0:?}")]
    Timeout(Duration),

    /// Provider returned HTTP 4xx (bad request, auth, model not found …).
    /// Not retryable — the caller must change the request.
    #[error("client error (status {status}): {message}")]
    ClientError { status: u16, message: String },

    /// We could not parse the provider's response (shape changed,
    /// truncated JSON, …).
    #[error("decode: {0}")]
    Decode(String),

    /// VCR fixture missing for a given request key. Tests only.
    #[error("vcr cassette miss for key `{0}`")]
    CassetteMiss(String),

    /// A provider was instantiated without the configuration it needs
    /// (e.g. `ANTHROPIC_API_KEY` unset).
    #[error("misconfigured provider: {0}")]
    Misconfigured(String),

    /// The adapter is intentionally not yet implemented. Used by the
    /// Google / Mistral / vLLM placeholders shipping in Phase 3.
    #[error("provider `{0}` not implemented (planned for v0.4)")]
    NotImplemented(&'static str),

    /// Generic I/O during VCR persistence, etc.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// Anything that goes through `serde_json` (response parsing, cassette
    /// serialisation).
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}

impl LlmError {
    /// Whether the orchestrator's [`RetryPolicy`] should consider this
    /// failure retryable.
    ///
    /// [`RetryPolicy`]: ../genatp_core/retry/struct.RetryPolicy.html
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Transport(_)
                | Self::RateLimited { .. }
                | Self::ServerError { .. }
                | Self::Timeout(_)
        )
    }

    /// Provider-supplied retry-after hint, if any. Used by the backoff
    /// policy to honour 429 responses correctly.
    #[must_use]
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::RateLimited { retry_after } => *retry_after,
            _ => None,
        }
    }
}

#[cfg(feature = "http")]
impl From<reqwest::Error> for LlmError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            return Self::Timeout(Duration::from_secs(0));
        }
        if err.is_decode() {
            return Self::Decode(err.to_string());
        }
        if let Some(status) = err.status() {
            let code = status.as_u16();
            return if status.is_server_error() {
                Self::ServerError {
                    status: code,
                    message: err.to_string(),
                }
            } else {
                Self::ClientError {
                    status: code,
                    message: err.to_string(),
                }
            };
        }
        Self::Transport(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_is_retryable() {
        assert!(LlmError::Transport("conn refused".into()).is_retryable());
    }

    #[test]
    fn client_error_is_not_retryable() {
        assert!(
            !LlmError::ClientError {
                status: 401,
                message: "auth".into(),
            }
            .is_retryable()
        );
    }

    #[test]
    fn rate_limit_returns_retry_after() {
        let err = LlmError::RateLimited {
            retry_after: Some(Duration::from_secs(3)),
        };
        assert!(err.is_retryable());
        assert_eq!(err.retry_after(), Some(Duration::from_secs(3)));
    }

    #[test]
    fn not_implemented_is_not_retryable() {
        assert!(!LlmError::NotImplemented("google").is_retryable());
    }
}
