//! Independent Stage 8 statistics reference checks.

use peritus_eval::{DistributionSummary, pass_at_k};
use proptest::prelude::*;

fn choose(n: u64, k: u64) -> u128 {
    let k = k.min(n - k);
    (0..k).fold(1_u128, |value, i| value * u128::from(n - i) / u128::from(i + 1))
}

fn exact_pass_millionths(n: u32, successes: u32, k: u16) -> u32 {
    let all = choose(u64::from(n), u64::from(k));
    let failed = if n - successes < u32::from(k) {
        0
    } else {
        choose(u64::from(n - successes), u64::from(k))
    };
    u32::try_from(((all - failed) * 1_000_000 + all / 2) / all).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 512,
        failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Direct(
            "crates/analysis/peritus-eval/tests/stage8_statistics.proptest-regressions",
        ))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn pass_at_k_matches_exact_small_combinations(
        (n, successes, k) in (1_u32..=64).prop_flat_map(|n|
            (Just(n), 0_u32..=n, 1_u16..=u16::try_from(n).unwrap()))
    ) {
        let observed = pass_at_k(n, successes, k).unwrap().estimate().get();
        let exact = exact_pass_millionths(n, successes, k);
        prop_assert_eq!(observed, exact,
            "n={} c={} k={}", n, successes, k);
    }

    #[test]
    fn distribution_matches_sorted_order_statistics(
        values in prop::collection::vec(0_u64..1_000_000, 1..80),
        missing in any::<u32>()
    ) {
        let observed = DistributionSummary::new(values.clone(), missing).unwrap();
        let mut reversed = values.clone();
        reversed.reverse();
        prop_assert_eq!(observed, DistributionSummary::new(reversed, missing).unwrap());
        let mut ordered = values;
        ordered.sort_unstable();
        let rank = |percent: usize| ordered[(ordered.len() * percent).div_ceil(100) - 1];
        let sum: u64 = ordered.iter().sum();
        prop_assert_eq!(observed.count(), u32::try_from(ordered.len()).unwrap());
        prop_assert_eq!(observed.missing(), missing);
        prop_assert_eq!(observed.total(), sum);
        prop_assert_eq!(observed.minimum(), ordered[0]);
        prop_assert_eq!(observed.maximum(), *ordered.last().unwrap());
        prop_assert_eq!(observed.mean(), sum / u64::try_from(ordered.len()).unwrap());
        prop_assert_eq!(observed.p50(), rank(50));
        prop_assert_eq!(observed.p95(), rank(95));
        prop_assert_eq!(observed.p99(), rank(99));
    }

    #[test]
    fn large_pass_at_k_remains_bounded_and_monotone(
        (n, c, k) in (1_u32..=u32::MAX).prop_flat_map(|n|
            (Just(n), 0_u32..=n, 1_u16..=u16::try_from(n.min(u32::from(u16::MAX))).unwrap()))
    ) {
        let observed = pass_at_k(n, c, k).unwrap().estimate().get();
        prop_assert!(observed <= 1_000_000);
        if c < n {
            let more_successes = pass_at_k(n, c + 1, k).unwrap().estimate().get();
            prop_assert!(more_successes >= observed);
        }
        if u32::from(k) < n && k < u16::MAX {
            let more_draws = pass_at_k(n, c, k + 1).unwrap().estimate().get();
            prop_assert!(more_draws >= observed);
        }
    }
}

#[test]
fn invalid_statistics_reject_without_panic() {
    for (n, c, k) in [(0, 0, 1), (1, 2, 1), (2, 1, 0), (2, 1, 3)] {
        assert!(pass_at_k(n, c, k).is_err());
    }
    assert!(DistributionSummary::new(Vec::new(), 0).is_err());
    assert!(DistributionSummary::new(vec![u64::MAX, 1], 0).is_err());
}

#[test]
fn fixed_rounding_and_nearest_rank_boundaries() {
    assert_eq!(pass_at_k(6, 1, 1).unwrap().estimate().get(), 166_667);
    let summary = DistributionSummary::new(vec![3, 1, 2], 0).unwrap();
    assert_eq!((summary.p50(), summary.p95(), summary.p99()), (2, 3, 3));
}
