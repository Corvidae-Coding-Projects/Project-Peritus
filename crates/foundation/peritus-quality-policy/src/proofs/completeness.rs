//! Structural evaluator-completeness proof obligations.

#[cfg(verus_only)]
use crate::model;
use vstd::prelude::*;
#[cfg(verus_only)]
use crate::AcceptanceDecision;

verus! {

/// A complete phase-status aggregation implies every evaluator phase reported success.
pub proof fn completeness_implies_all_obligations(
    contract_bound: bool,
    observations_fresh: bool,
    gates_complete: bool,
    evidence_complete: bool,
    reviews_complete: bool,
    blockers_complete: bool,
    approvals_complete: bool,
)
    requires model::acceptance_complete(
        contract_bound,
        observations_fresh,
        gates_complete,
        evidence_complete,
        reviews_complete,
        blockers_complete,
        approvals_complete,
    ),
    ensures
        contract_bound,
        observations_fresh,
        gates_complete,
        evidence_complete,
        reviews_complete,
        blockers_complete,
        approvals_complete,
{}

/// No single incomplete phase status can be converted into a complete aggregation.
pub proof fn incomplete_obligation_prevents_acceptance(
    contract_bound: bool,
    observations_fresh: bool,
    gates_complete: bool,
    evidence_complete: bool,
    reviews_complete: bool,
    blockers_complete: bool,
    approvals_complete: bool,
)
    requires
        !contract_bound
            || !observations_fresh
            || !gates_complete
            || !evidence_complete
            || !reviews_complete
            || !blockers_complete
            || !approvals_complete,
    ensures !model::acceptance_complete(
        contract_bound,
        observations_fresh,
        gates_complete,
        evidence_complete,
        reviews_complete,
        blockers_complete,
        approvals_complete,
    ),
{}

/// The executable decision type cannot report acceptable unless every evaluator phase reports
/// success and no typed unmet condition remains.
pub(crate) proof fn executable_acceptance_implies_complete(decision: &AcceptanceDecision)
    requires decision.spec_is_acceptable(),
    ensures
        decision.spec_checks_complete(),
        decision.spec_unmet_conditions().len() == 0,
{
    decision.accepted_has_complete_checks();
    decision.accepted_has_no_unmet_conditions();
}

} // verus!
