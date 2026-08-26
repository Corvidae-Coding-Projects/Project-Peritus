//! Executable refinement checks for every E3 Verus proof root.

use peritus_eval::verified::{
    FrozenProfileFacts, LedgerValidityFacts, NonAuthorityFacts, ReplayRefinementFacts,
    StatisticalValidityFacts, TransitionValidityFacts, accounting_conserved, frozen_profile,
    ledger_validity, non_authority, pass_at_k_preconditions, replay_refinement,
    statistical_validity, terminal_dominates, transition_validity,
};

#[test]
fn accounting_and_statistical_preconditions_match_runtime_boundaries() {
    assert!(accounting_conserved(8, 5, 1, 1, 1, 0));
    assert!(!accounting_conserved(8, 5, 1, 1, 0, 0));
    assert!(pass_at_k_preconditions(4, 2, 2));
    assert!(!pass_at_k_preconditions(4, 5, 2));
    assert!(statistical_validity(StatisticalValidityFacts::new(
        true, true, true, true, true, true, true,
    )));
    assert!(!statistical_validity(StatisticalValidityFacts::new(
        true, true, false, true, true, true, true,
    )));
}

#[test]
fn frozen_ledger_transition_replay_and_non_authority_roots_are_exact_conjunctions() {
    assert!(frozen_profile(FrozenProfileFacts::new(true, true, true, true, true, true,)));
    assert!(ledger_validity(LedgerValidityFacts::new(true, true, true, true, true, true, true,)));
    assert!(transition_validity(TransitionValidityFacts::new(true, true, true, true, true, true,)));
    assert!(replay_refinement(ReplayRefinementFacts::new(true, true, true, true, true, true,)));
    assert!(non_authority(NonAuthorityFacts::new(true, true, true, true, true, true,)));
    assert!(terminal_dominates(false, false));
    assert!(terminal_dominates(true, true));
    assert!(!terminal_dominates(true, false));

    assert!(!ledger_validity(LedgerValidityFacts::new(true, true, false, true, true, true, true,)));
    assert!(!replay_refinement(ReplayRefinementFacts::new(true, true, true, true, false, true,)));
    assert!(!non_authority(NonAuthorityFacts::new(true, true, false, true, true, true,)));
}
