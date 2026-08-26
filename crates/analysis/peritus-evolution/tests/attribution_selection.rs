//! Attribution, frozen-policy assessment, and deterministic selection tests.

use peritus_evolution::{
    EvolutionErrorKind, EvolutionLimits, Objective, PromotionPolicy, PromotionThresholds,
    verified::deny_wins,
};
use peritus_harness::domain::ComponentKind;

fn thresholds() -> PromotionThresholds {
    PromotionThresholds::new(
        -10_000, 0, 0, 900_000, 1_000_000, 2_000_000, 1_000_000, 16_000, 8_000, true, true,
    )
    .expect("valid thresholds")
}

#[test]
fn frozen_objective_order_is_policy_order_not_enum_order() {
    let policy = PromotionPolicy::new(
        thresholds(),
        vec![Objective::Cost, Objective::PairedCorrectness, Objective::Reliability],
        vec![ComponentKind::RoleDefinition],
        false,
        3,
        EvolutionLimits::compiled(),
    )
    .expect("a unique caller-selected objective order is valid");

    assert_eq!(
        policy.objectives(),
        &[Objective::Cost, Objective::PairedCorrectness, Objective::Reliability]
    );
}

#[test]
fn duplicate_objectives_are_rejected_and_unavailable_denies() {
    let error = PromotionPolicy::new(
        thresholds(),
        vec![Objective::Cost, Objective::Cost],
        Vec::new(),
        false,
        1,
        EvolutionLimits::compiled(),
    )
    .expect_err("objective identities must be unique");
    assert_eq!(error.kind(), EvolutionErrorKind::NonCanonical);
    assert!(!deny_wins(true, 0, 1));
    assert!(!deny_wins(true, 1, 0));
    assert!(deny_wins(true, 0, 0));
}
