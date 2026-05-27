//! Property-based tests for the budget guard.
//!
//! Invariants:
//! 1. `tokens_spent` is monotonically non-decreasing.
//! 2. `sum(usage.total_tokens) ≤ max_tokens` once Halt is observed
//!    (the halt step itself may push spend past the limit; assertion is
//!    that we never silently keep going past it).
//! 3. Soft warn fires strictly before hard halt.

use genatp_core::budget::{BudgetCheck, BudgetGuard, BudgetLimits, Pricing};
use genatp_llm::Usage;
use proptest::prelude::*;

fn usage_strat() -> impl Strategy<Value = Usage> {
    (0u32..200u32, 0u32..200u32).prop_map(|(p, c)| Usage {
        prompt_tokens: p,
        completion_tokens: c,
        total_tokens: p.saturating_add(c),
    })
}

proptest! {
    #[test]
    fn tokens_are_monotonic(steps in prop::collection::vec(usage_strat(), 0..50)) {
        let mut g = BudgetGuard::new(BudgetLimits::default(), Pricing::FREE).unwrap();
        let mut prev = 0u32;
        for u in steps {
            g.record_step(u);
            prop_assert!(g.spend().tokens >= prev, "tokens decreased");
            prev = g.spend().tokens;
        }
    }

    #[test]
    fn halt_is_terminal_until_check(steps in prop::collection::vec(usage_strat(), 0..50), limit in 50u32..500u32) {
        let lim = BudgetLimits { max_tokens: limit, ..BudgetLimits::default() };
        let mut g = BudgetGuard::new(lim, Pricing::FREE).unwrap();
        let mut halted = false;
        for u in steps {
            let check = g.record_step(u);
            if matches!(check, BudgetCheck::Halt(_)) {
                halted = true;
            }
            if halted {
                // Once halted, subsequent checks must also halt (spend only grows).
                prop_assert!(matches!(g.check(), BudgetCheck::Halt(_)));
            }
        }
    }

    #[test]
    fn soft_strictly_before_hard(limit in 100u32..1000u32, soft_frac in (0.1f64..0.99f64)) {
        let lim = BudgetLimits { max_tokens: limit, soft_fraction: soft_frac, ..BudgetLimits::default() };
        let mut g = BudgetGuard::new(lim, Pricing::FREE).unwrap();
        // Pump one token at a time until hard fires; verify a soft fired earlier.
        let mut saw_soft = false;
        let mut saw_hard = false;
        for _ in 0..(limit as usize + 5) {
            let c = g.record_step(Usage { prompt_tokens: 1, completion_tokens: 0, total_tokens: 1 });
            match c {
                BudgetCheck::Warn(_) => saw_soft = true,
                BudgetCheck::Halt(_) => {
                    saw_hard = true;
                    break;
                }
                BudgetCheck::Ok => {}
            }
        }
        prop_assert!(saw_hard, "hard limit must fire eventually");
        prop_assert!(saw_soft, "soft warn must fire before hard");
    }
}
