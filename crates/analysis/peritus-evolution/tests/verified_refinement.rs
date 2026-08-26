//! Executable witnesses for F0's Verus refinement boundary.

use peritus_evolution::verified::{
    EvaluatorIsolationFacts, PromotionSafetyFacts, evaluator_isolation, pointer_conservation,
    promotion_safety, replay_refinement, rollback_legality, terminal_dominance,
};

#[test]
fn evaluator_isolation_requires_every_checked_boundary_fact() {
    let complete = EvaluatorIsolationFacts::new(true, true, true, true, true, true);
    let contaminated = EvaluatorIsolationFacts::new(true, true, false, true, true, true);
    assert!(evaluator_isolation(complete));
    assert!(!evaluator_isolation(contaminated));
}

#[test]
fn promotion_safety_is_a_conjunction_not_a_score() {
    let safe =
        PromotionSafetyFacts::new(true, true, true, true, true, true, true, true, true, true);
    let stale =
        PromotionSafetyFacts::new(true, false, true, true, true, true, true, true, true, true);
    assert!(promotion_safety(safe));
    assert!(!promotion_safety(stale));
}

#[test]
fn rollback_replay_and_terminal_refinements_fail_closed() {
    assert!(rollback_legality(true, true, true, true, true, true));
    assert!(!rollback_legality(true, false, true, true, true, true));
    assert!(pointer_conservation(true, true, true, true));
    assert!(!pointer_conservation(true, false, true, true));
    assert!(replay_refinement(true, true, true, true, true));
    assert!(!replay_refinement(true, true, false, true, true));
    assert!(terminal_dominance(true, true));
    assert!(!terminal_dominance(true, false));
}
