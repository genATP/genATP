//! Retry policy with exponential backoff and full jitter.
//!
//! Used by the orchestrator to retry transient LLM and browser failures.
//! Permanent failures (4xx, validation, budget exceeded) bypass retry.
//!
//! The backoff formula is:
//!
//! ```text
//! delay_n = min(max_delay, base * factor^n) * jitter
//! ```
//!
//! where `jitter ∈ [0.0, 1.0)` is uniform random ("full jitter" — the
//! pattern AWS recommends to prevent retry storms).
//!
//! Properties (verified in `tests/retry_property.rs`):
//!
//! - `delay_n ≤ max_delay` for all `n`.
//! - Total cumulative delay ≤ `max_total_delay`.
//! - The policy retries at most `max_attempts - 1` times.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Configurable retry policy.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Total attempts including the first try.
    pub max_attempts: u32,
    /// First retry's base delay.
    pub base_delay: Duration,
    /// Exponential multiplier between successive retries (`2.0` = double).
    pub factor: f64,
    /// Upper bound on any single retry's delay.
    pub max_delay: Duration,
    /// Upper bound on cumulative delay across all retries.
    pub max_total_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay: Duration::from_millis(250),
            factor: 2.0,
            max_delay: Duration::from_secs(8),
            max_total_delay: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    /// A no-retry policy: one attempt, no delay.
    pub const NEVER: Self = Self {
        max_attempts: 1,
        base_delay: Duration::ZERO,
        factor: 1.0,
        max_delay: Duration::ZERO,
        max_total_delay: Duration::ZERO,
    };

    /// Compute the delay before attempt `n` (where `n = 0` is the first
    /// attempt, `n = 1` is the first retry, …). Includes full jitter via
    /// the supplied PRNG.
    ///
    /// Returns `Duration::ZERO` for `n == 0`.
    #[must_use]
    pub fn delay_for(&self, attempt: u32, rng: &mut fastrand::Rng) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }
        let n = i32::try_from(attempt - 1).unwrap_or(i32::MAX);
        // base * factor^n
        let scaled = self.base_delay.as_secs_f64() * self.factor.powi(n);
        let capped = scaled.min(self.max_delay.as_secs_f64());
        let jitter = rng.f64();
        let final_secs = (capped * jitter).max(0.0);
        Duration::from_secs_f64(final_secs)
    }

    /// Whether the policy allows another attempt given the current attempt
    /// counter and the cumulative delay accrued so far.
    #[must_use]
    pub fn should_retry(&self, attempt: u32, cumulative: Duration) -> bool {
        attempt + 1 < self.max_attempts && cumulative <= self.max_total_delay
    }

    /// Validate the configuration.
    ///
    /// # Errors
    /// Returns a description when any field is logically inconsistent.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_attempts == 0 {
            return Err("max_attempts must be ≥ 1".into());
        }
        if !self.factor.is_finite() || self.factor < 1.0 {
            return Err(format!(
                "factor must be a finite number ≥ 1.0; got {}",
                self.factor
            ));
        }
        if self.max_delay < self.base_delay {
            return Err(format!(
                "max_delay ({:?}) must be ≥ base_delay ({:?})",
                self.max_delay, self.base_delay
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_validates() {
        RetryPolicy::default().validate().unwrap();
    }

    #[test]
    fn never_policy_disallows_retries() {
        assert!(!RetryPolicy::NEVER.should_retry(0, Duration::ZERO));
    }

    #[test]
    fn first_attempt_has_zero_delay() {
        let policy = RetryPolicy::default();
        let mut rng = fastrand::Rng::with_seed(42);
        assert_eq!(policy.delay_for(0, &mut rng), Duration::ZERO);
    }

    #[test]
    fn delay_capped_at_max_delay() {
        let policy = RetryPolicy {
            base_delay: Duration::from_secs(1),
            factor: 10.0,
            max_delay: Duration::from_secs(5),
            ..RetryPolicy::default()
        };
        let mut rng = fastrand::Rng::with_seed(7);
        for n in 1..10 {
            let d = policy.delay_for(n, &mut rng);
            assert!(
                d <= policy.max_delay,
                "attempt {n} delay {:?} > {:?}",
                d,
                policy.max_delay
            );
        }
    }

    #[test]
    fn should_retry_stops_after_max_attempts() {
        let policy = RetryPolicy {
            max_attempts: 3,
            ..RetryPolicy::default()
        };
        assert!(policy.should_retry(0, Duration::ZERO));
        assert!(policy.should_retry(1, Duration::ZERO));
        assert!(!policy.should_retry(2, Duration::ZERO));
    }

    #[test]
    fn should_retry_stops_when_total_delay_exceeded() {
        let policy = RetryPolicy {
            max_total_delay: Duration::from_secs(1),
            ..RetryPolicy::default()
        };
        assert!(!policy.should_retry(0, Duration::from_secs(10)));
    }

    #[test]
    fn factor_less_than_one_is_invalid() {
        let p = RetryPolicy {
            factor: 0.5,
            ..RetryPolicy::default()
        };
        assert!(p.validate().is_err());
    }
}
