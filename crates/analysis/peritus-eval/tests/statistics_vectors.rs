//! Published deterministic statistical vector coverage.

mod support;

use peritus_eval::{
    DistributionSummary, PairedCell, StabilityClass, TaskId, WilsonInterval, analyze_stability,
    compare_paired, pass_at_k,
};

use support::{bytes, digest};

#[test]
fn pass_at_k_matches_exact_combinatorial_edges() {
    assert_eq!(pass_at_k(10, 0, 2).expect("zero successes").estimate().get(), 0);
    assert_eq!(pass_at_k(10, 2, 1).expect("pass@1").estimate().get(), 200_000);
    assert_eq!(pass_at_k(10, 2, 2).expect("pass@2").estimate().get(), 377_778);
    assert_eq!(pass_at_k(3, 2, 2).expect("certain pass").estimate().get(), 1_000_000);
    assert!(pass_at_k(2, 1, 3).is_err());
}

#[test]
fn wilson_distribution_and_stability_retain_raw_inputs() {
    let interval = WilsonInterval::ninety_five(5, 10).expect("Wilson interval");
    assert_eq!(interval.successes(), 5);
    assert_eq!(interval.total(), 10);
    assert!(interval.lower().get() < 500_000);
    assert!(interval.upper().get() > 500_000);

    let distribution = DistributionSummary::new(vec![10, 20, 30, 40, 50], 2).expect("summary");
    assert_eq!(distribution.total(), 150);
    assert_eq!(distribution.p50(), 30);
    assert_eq!(distribution.p95(), 50);
    assert_eq!(distribution.missing(), 2);

    let stability = analyze_stability(&[true, true, false, true], 250_000).expect("stability");
    assert_eq!(stability.transitions(), 2);
    assert_eq!(stability.longest_pass_streak(), 2);
    assert_eq!(stability.class(), StabilityClass::Unstable);
}

#[test]
fn paired_bootstrap_and_sign_diagnostic_are_reproducible() {
    let task_a = TaskId::new(bytes(10)).expect("task");
    let task_b = TaskId::new(bytes(11)).expect("task");
    let cells = vec![
        PairedCell::new(task_a, 1, false, true).expect("cell"),
        PairedCell::new(task_a, 2, true, true).expect("cell"),
        PairedCell::new(task_b, 1, true, false).expect("cell"),
        PairedCell::new(task_b, 2, false, false).expect("cell"),
    ];
    let profile = peritus_eval::ProfileDigest::new(digest(90));
    let left = compare_paired(profile, &cells, 200, 950_000).expect("paired");
    let right = compare_paired(profile, &cells, 200, 950_000).expect("paired");
    assert_eq!(left, right);
    assert_eq!(left.table().candidate_only, 1);
    assert_eq!(left.table().baseline_only, 1);
    assert_eq!(left.net_effect_millionths(), 0);
    assert_eq!(left.sign_test().positive_tasks(), 1);
    assert_eq!(left.sign_test().negative_tasks(), 1);
}
