//! Structural H0 readiness proof obligations.

#[cfg(verus_only)]
use crate::{SecurityDecision, model};
use vstd::prelude::*;

verus! {

/// Readiness aggregation implies every required H0 phase is complete.
pub proof fn readiness_implies_all_security_obligations(
    candidate_bound: bool,
    requirements_complete: bool,
    criteria_complete: bool,
    inventories_complete: bool,
    independent_review_complete: bool,
    blockers_clear: bool,
    evidence_complete: bool,
)
    requires model::security_ready(
        candidate_bound,
        requirements_complete,
        criteria_complete,
        inventories_complete,
        independent_review_complete,
        blockers_clear,
        evidence_complete,
    ),
    ensures
        candidate_bound,
        requirements_complete,
        criteria_complete,
        inventories_complete,
        independent_review_complete,
        blockers_clear,
        evidence_complete,
{}

/// Any incomplete H0 phase prevents readiness.
pub proof fn incomplete_security_obligation_prevents_readiness(
    candidate_bound: bool,
    requirements_complete: bool,
    criteria_complete: bool,
    inventories_complete: bool,
    independent_review_complete: bool,
    blockers_clear: bool,
    evidence_complete: bool,
)
    requires
        !candidate_bound
            || !requirements_complete
            || !criteria_complete
            || !inventories_complete
            || !independent_review_complete
            || !blockers_clear
            || !evidence_complete,
    ensures !model::security_ready(
        candidate_bound,
        requirements_complete,
        criteria_complete,
        inventories_complete,
        independent_review_complete,
        blockers_clear,
        evidence_complete,
    ),
{}

/// Ordinary decision semantics cannot report ready without complete checks and empty failures.
pub(crate) proof fn executable_readiness_implies_complete(decision: &SecurityDecision)
    requires decision.spec_is_ready(),
    ensures
        decision.spec_checks_complete(),
        decision.spec_unmet_conditions().len() == 0,
{
    decision.ready_has_complete_checks();
    decision.ready_has_all_security_obligations();
    decision.ready_has_no_unmet_conditions();
}

} // verus!
