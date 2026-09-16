//! Improvement thresholds retain regressions when applying an admitted noise margin.

mod support;

use peritus_obligations::{
    ObligationSpec, PerformanceEvidence, PerformanceExpectation, PerformanceRequirement,
    PerformanceStatistic, RequirementEvidence, qualify,
};
use support::{binding, candidate, digest, ledger};

#[test]
fn improvement_noise_cannot_hide_a_large_regression() {
    let candidate = candidate(11, 7, 2);
    let threshold = PerformanceExpectation::ImprovementAtLeast(5);
    let requirement =
        PerformanceRequirement::new(digest(31), PerformanceStatistic::Median, 5, threshold)
            .expect("performance requirement");
    let ledger = ledger(vec![(
        1,
        b"The candidate must improve the baseline by five units, with ten units of noise.",
        ObligationSpec::Performance(requirement),
        Vec::new(),
    )]);
    let evidence = PerformanceEvidence::new(
        binding(&ledger, candidate, 1, Vec::new(), 41),
        digest(31),
        100,
        1_000,
        5,
        PerformanceStatistic::Median,
        10,
        threshold,
    )
    .expect("performance evidence");
    let report = qualify(&ledger, &candidate, &[], &[RequirementEvidence::Performance(evidence)])
        .expect("qualification report");
    assert!(!report.qualified(), "a 900-unit regression exceeds a ten-unit noise margin");
    assert_eq!(report.invalid_count(), 1);
}

#[test]
fn improvement_preserves_inclusive_and_full_width_boundaries() {
    let candidate_id = candidate(11, 7, 2);
    let ledger =
        ledger(vec![(1, b"A public performance requirement.", ObligationSpec::Hard, Vec::new())]);
    for (baseline, measured, noise, improvement, accepted) in [
        (100, 95, 0, 5, true),
        (100, 96, 0, 5, false),
        (100, 105, 10, 5, true),
        (100, 106, 10, 5, false),
        (100, 101, 0, 0, false),
        (100, 100, 5, 5, true),
        (0, 0, 0, 0, true),
        (u64::MAX, 0, u64::MAX, u64::MAX, true),
        (0, u64::MAX, u64::MAX, 1, false),
        (u64::MAX, u64::MAX, u64::MAX, u64::MAX, true),
        (u64::MAX, u64::MAX - 1, 0, 1, true),
    ] {
        let threshold = PerformanceExpectation::ImprovementAtLeast(improvement);
        let requirement =
            PerformanceRequirement::new(digest(31), PerformanceStatistic::Median, 5, threshold)
                .expect("requirement");
        let evidence = PerformanceEvidence::new(
            binding(&ledger, candidate_id, 1, Vec::new(), 41),
            digest(31),
            baseline,
            measured,
            5,
            PerformanceStatistic::Median,
            noise,
            threshold,
        )
        .expect("evidence");
        assert_eq!(
            evidence.satisfies(requirement),
            accepted,
            "baseline={baseline} candidate={measured} noise={noise} improvement={improvement}"
        );
        assert_eq!(evidence.clone(), evidence);
        assert_eq!(evidence.clone().satisfies(requirement), accepted);
    }
}
