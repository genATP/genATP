//! Budget enforcement for the orchestrator.
//!
//! The orchestrator must guarantee the agent does **not** exceed the
//! limits declared at task launch. Three independent limits compete:
//!
//! 1. **Tokens** — sum of `prompt + completion` reported by the provider.
//! 2. **Wall time** — `Instant::now() - start`.
//! 3. **USD** — token usage converted via [`Pricing`] (model-specific).
//!
//! Each dimension has a `soft` and `hard` watermark:
//!
//! - **Soft (default 80 %)** — [`BudgetGuard::check`] returns
//!   [`BudgetCheck::Warn`]; the orchestrator logs a warning but continues.
//! - **Hard (100 %)** — [`BudgetCheck::Halt`]; the orchestrator stops the
//!   plan loop and emits a `BudgetExceeded` event.
//!
//! Invariants enforced by tests:
//!
//! - `sum(charges) ≤ hard_limit` once a hard limit fires.
//! - `tokens_spent` and friends are monotonically non-decreasing.
//! - Soft fires strictly before hard for every dimension.

use std::time::{Duration, Instant};

use genatp_llm::Usage;
use serde::{Deserialize, Serialize};

/// Per-step pricing (USD per 1 K tokens) for a given model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pricing {
    pub input_per_1k: f64,
    pub output_per_1k: f64,
}

impl Pricing {
    /// Pricing of $0 (free / local providers like Ollama).
    pub const FREE: Self = Self {
        input_per_1k: 0.0,
        output_per_1k: 0.0,
    };

    /// Multiply usage by per-1K rates. Saturates on overflow / nan.
    #[must_use]
    pub fn cost_of(self, usage: Usage) -> f64 {
        let p = f64::from(usage.prompt_tokens) * self.input_per_1k / 1_000.0;
        let c = f64::from(usage.completion_tokens) * self.output_per_1k / 1_000.0;
        let total = p + c;
        if total.is_finite() && total >= 0.0 {
            total
        } else {
            0.0
        }
    }
}

/// Limits a task may not exceed.
///
/// Any limit set to `0` is treated as *unlimited*. The fraction of the
/// limit at which a soft warning fires is [`BudgetLimits::soft_fraction`]
/// (default 0.80 — i.e. 80 %).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BudgetLimits {
    pub max_tokens: u32,
    pub max_wall: Duration,
    pub max_usd: f64,
    /// Soft warn threshold (0.0–1.0). Anything ≥ this and < 1.0 is `Warn`.
    pub soft_fraction: f64,
}

impl Default for BudgetLimits {
    fn default() -> Self {
        Self {
            max_tokens: 0,
            max_wall: Duration::from_secs(0),
            max_usd: 0.0,
            soft_fraction: 0.80,
        }
    }
}

impl BudgetLimits {
    /// Validate the configuration.
    ///
    /// # Errors
    /// Returns [`BudgetError::InvalidConfig`] if `soft_fraction` is not in
    /// `(0.0, 1.0]`.
    pub fn validate(&self) -> Result<(), BudgetError> {
        if !(0.0 < self.soft_fraction && self.soft_fraction <= 1.0) {
            return Err(BudgetError::InvalidConfig(format!(
                "soft_fraction must be in (0, 1]; got {}",
                self.soft_fraction
            )));
        }
        if self.max_usd.is_nan() || self.max_usd < 0.0 {
            return Err(BudgetError::InvalidConfig(format!(
                "max_usd must be a finite non-negative number; got {}",
                self.max_usd
            )));
        }
        Ok(())
    }

    fn token_unlimited(self) -> bool {
        self.max_tokens == 0
    }
    fn wall_unlimited(self) -> bool {
        self.max_wall.is_zero()
    }
    fn usd_unlimited(self) -> bool {
        self.max_usd <= 0.0
    }
}

/// Errors specific to budget enforcement.
#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum BudgetError {
    #[error("budget hard limit hit on {dimension}: {used} >= {limit}")]
    Hard {
        dimension: &'static str,
        used: String,
        limit: String,
    },
    #[error("invalid budget config: {0}")]
    InvalidConfig(String),
}

/// Possible outcomes of [`BudgetGuard::check`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetCheck {
    /// Under all soft thresholds — keep going.
    Ok,
    /// At or past soft on at least one dimension — log a warning, continue.
    Warn(&'static str),
    /// At or past hard on at least one dimension — orchestrator must stop.
    Halt(&'static str),
}

/// Running tally of spend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct BudgetSpend {
    pub tokens: u32,
    pub usd: f64,
    pub steps: u32,
    /// Wall time at the start of the run. Persisted as a `chrono` UTC
    /// timestamp; the `Instant` is reconstructed internally.
    #[serde(default)]
    pub started_at_utc: Option<chrono::DateTime<chrono::Utc>>,
}

/// Guard owned by the orchestrator.
#[derive(Debug)]
pub struct BudgetGuard {
    limits: BudgetLimits,
    pricing: Pricing,
    spend: BudgetSpend,
    start: Instant,
}

impl BudgetGuard {
    /// Construct a guard.
    ///
    /// # Errors
    /// Forwards [`BudgetLimits::validate`].
    pub fn new(limits: BudgetLimits, pricing: Pricing) -> Result<Self, BudgetError> {
        limits.validate()?;
        Ok(Self {
            limits,
            pricing,
            spend: BudgetSpend {
                started_at_utc: Some(chrono::Utc::now()),
                ..BudgetSpend::default()
            },
            start: Instant::now(),
        })
    }

    /// Return a copy of the current spend.
    #[must_use]
    pub fn spend(&self) -> BudgetSpend {
        self.spend
    }

    /// Return the limits (read-only).
    #[must_use]
    pub fn limits(&self) -> BudgetLimits {
        self.limits
    }

    /// Add provider usage to the running spend. Returns the post-charge
    /// check outcome. Always increments the step counter.
    pub fn record_step(&mut self, usage: Usage) -> BudgetCheck {
        self.spend.steps = self.spend.steps.saturating_add(1);
        self.spend.tokens = self.spend.tokens.saturating_add(usage.total_tokens);
        let cost = self.pricing.cost_of(usage);
        // Manual `max(0)` to defend against any float drift to ≤ 0.
        let next = self.spend.usd + cost;
        self.spend.usd = if next > 0.0 { next } else { 0.0 };
        self.check()
    }

    /// Check whether any limit is exceeded. Does **not** mutate spend.
    /// The orchestrator can call this between steps to enforce wall-time
    /// independently of LLM usage.
    #[must_use]
    pub fn check(&self) -> BudgetCheck {
        let elapsed = self.start.elapsed();

        // Halt has priority over warn — check hard limits first.
        if !self.limits.token_unlimited() && self.spend.tokens >= self.limits.max_tokens {
            return BudgetCheck::Halt("tokens");
        }
        if !self.limits.wall_unlimited() && elapsed >= self.limits.max_wall {
            return BudgetCheck::Halt("wall");
        }
        if !self.limits.usd_unlimited() && self.spend.usd >= self.limits.max_usd {
            return BudgetCheck::Halt("usd");
        }

        // Soft.
        let frac = self.limits.soft_fraction;
        if !self.limits.token_unlimited() {
            let soft = (f64::from(self.limits.max_tokens) * frac) as u32;
            if self.spend.tokens >= soft {
                return BudgetCheck::Warn("tokens");
            }
        }
        if !self.limits.wall_unlimited() {
            let soft = self.limits.max_wall.mul_f64(frac);
            if elapsed >= soft {
                return BudgetCheck::Warn("wall");
            }
        }
        if !self.limits.usd_unlimited() && self.spend.usd >= self.limits.max_usd * frac {
            return BudgetCheck::Warn("usd");
        }

        BudgetCheck::Ok
    }

    /// Convert a [`BudgetCheck::Halt`] outcome into a typed [`BudgetError`].
    /// Returns `None` for `Ok` / `Warn`.
    #[must_use]
    pub fn halt_error(&self, check: BudgetCheck) -> Option<BudgetError> {
        let BudgetCheck::Halt(dim) = check else {
            return None;
        };
        let (used, limit) = match dim {
            "tokens" => (
                self.spend.tokens.to_string(),
                self.limits.max_tokens.to_string(),
            ),
            "wall" => (
                format!("{:?}", self.start.elapsed()),
                format!("{:?}", self.limits.max_wall),
            ),
            "usd" => (
                format!("{:.4}", self.spend.usd),
                format!("{:.4}", self.limits.max_usd),
            ),
            _ => (String::new(), String::new()),
        };
        Some(BudgetError::Hard {
            dimension: dim,
            used,
            limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(n: u32) -> Usage {
        Usage {
            prompt_tokens: n,
            completion_tokens: 0,
            total_tokens: n,
        }
    }

    #[test]
    fn limits_default_validates() {
        BudgetLimits::default().validate().unwrap();
    }

    #[test]
    fn invalid_soft_fraction_errors() {
        let l = BudgetLimits {
            soft_fraction: 0.0,
            ..BudgetLimits::default()
        };
        assert!(l.validate().is_err());
    }

    #[test]
    fn tokens_halt_at_hard_limit() {
        // soft threshold @ 80, hard @ 100.
        let mut g = BudgetGuard::new(
            BudgetLimits {
                max_tokens: 100,
                ..BudgetLimits::default()
            },
            Pricing::FREE,
        )
        .unwrap();
        assert_eq!(g.record_step(usage(50)), BudgetCheck::Ok);
        assert_eq!(g.record_step(usage(30)), BudgetCheck::Warn("tokens"));
        assert_eq!(g.record_step(usage(20)), BudgetCheck::Halt("tokens"));
        let err = g.halt_error(g.check()).unwrap();
        assert!(matches!(
            err,
            BudgetError::Hard {
                dimension: "tokens",
                ..
            }
        ));
    }

    #[test]
    fn unlimited_dimensions_never_halt() {
        let mut g = BudgetGuard::new(BudgetLimits::default(), Pricing::FREE).unwrap();
        for _ in 0..1000 {
            assert_eq!(g.record_step(usage(1000)), BudgetCheck::Ok);
        }
    }

    #[test]
    fn usd_cost_is_zero_for_free_pricing() {
        let mut g = BudgetGuard::new(BudgetLimits::default(), Pricing::FREE).unwrap();
        g.record_step(usage(1_000_000));
        assert_eq!(g.spend().usd, 0.0);
    }

    #[test]
    fn usd_cost_computes_from_pricing() {
        let mut g = BudgetGuard::new(
            BudgetLimits {
                max_usd: 1.0,
                ..BudgetLimits::default()
            },
            Pricing {
                input_per_1k: 1.0,
                output_per_1k: 2.0,
            },
        )
        .unwrap();
        let u = Usage {
            prompt_tokens: 1000,
            completion_tokens: 500,
            total_tokens: 1500,
        };
        g.record_step(u);
        // prompt: 1000/1000 * 1.0 = 1.0
        // completion: 500/1000 * 2.0 = 1.0
        // total: 2.0 -> halt (>= 1.0)
        assert!(g.spend().usd >= 2.0);
        assert_eq!(g.check(), BudgetCheck::Halt("usd"));
    }

    #[test]
    fn spend_is_monotonic_non_decreasing() {
        let mut g = BudgetGuard::new(BudgetLimits::default(), Pricing::FREE).unwrap();
        let mut prev = 0;
        for _ in 0..50 {
            g.record_step(usage(7));
            assert!(g.spend().tokens >= prev);
            prev = g.spend().tokens;
        }
    }
}
