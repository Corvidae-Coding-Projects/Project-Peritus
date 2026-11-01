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
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/stage8_statistics.proptest-regressions"),
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

    #[test]
    fn distribution_checks_wide_totals_and_exact_overflow_boundary(
        left in 4_u64..=u64::MAX - 3,
        delta in -3_i8..=3,
        zeros in 0_usize..=120,
        missing in any::<u32>(),
    ) {
        let expected_total = u128::from(u64::MAX).checked_add_signed(i128::from(delta)).unwrap();
        let right = u64::try_from(expected_total - u128::from(left)).unwrap();
        let mut values = vec![left, right];
        values.resize(zeros + 2, 0);
        let observed = DistributionSummary::new(values.clone(), missing);
        if expected_total > u128::from(u64::MAX) {
            prop_assert!(observed.is_err(), "overflow was accepted: {expected_total}");
        } else {
            let observed = observed.unwrap();
            prop_assert_eq!(u128::from(observed.total()), expected_total);
            prop_assert_eq!(u128::from(observed.mean()), expected_total / values.len() as u128);
            prop_assert_eq!(observed.missing(), missing);
            // Independent cumulative-frequency oracle, including many tied zero samples.
            for (percent, actual) in [(50, observed.p50()), (95, observed.p95()), (99, observed.p99())] {
                let expected = values.iter().copied().filter(|candidate|
                    values.iter().filter(|value| *value <= candidate).count() * 100 >= values.len() * percent
                ).min().unwrap();
                prop_assert_eq!(actual, expected);
            }
            values.reverse();
            prop_assert_eq!(DistributionSummary::new(values, missing).unwrap(), observed);
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
