//! Property tests for the retry policy.

use std::time::Duration;

use genatp_core::retry::RetryPolicy;
use proptest::prelude::*;

proptest! {
    #[test]
    fn delay_never_exceeds_max_delay(
        seed in any::<u64>(),
        base_ms in 1u64..1_000u64,
        max_ms in 1u64..30_000u64,
        attempts in 1u32..20u32,
    ) {
        prop_assume!(max_ms >= base_ms);
        let policy = RetryPolicy {
            max_attempts: 10,
            base_delay: Duration::from_millis(base_ms),
            factor: 2.0,
            max_delay: Duration::from_millis(max_ms),
            max_total_delay: Duration::from_secs(3_600),
        };
        let mut rng = fastrand::Rng::with_seed(seed);
        for n in 0..attempts {
            let d = policy.delay_for(n, &mut rng);
            prop_assert!(d <= policy.max_delay, "{:?} > {:?}", d, policy.max_delay);
        }
    }

    #[test]
    fn should_retry_respects_max_attempts(max in 1u32..10u32, attempt in 0u32..20u32) {
        let policy = RetryPolicy { max_attempts: max, ..RetryPolicy::default() };
        let allowed = policy.should_retry(attempt, Duration::ZERO);
        prop_assert_eq!(allowed, attempt + 1 < max);
    }
}
