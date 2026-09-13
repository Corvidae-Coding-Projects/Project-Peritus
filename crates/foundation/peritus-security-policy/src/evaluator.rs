//! Total effect-free security-readiness evaluation.

mod artifacts;
mod binding;
mod controls;
mod inventories;
mod review;

use crate::decision::{CheckResult, SecurityChecks};
use crate::{IntegratedCandidate, SecurityDecision, SecurityEvidence, UnmetSecurityCondition};
use vstd::prelude::*;

verus! {

/// Formal contract established by every ready result.
pub open spec fn ready_evaluation_contract(
    decision: &SecurityDecision,
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
) -> bool {
    decision.spec_is_ready() ==> {
        &&& evidence.spec_all_current(candidate)
        &&& decision.spec_checks_complete()
        &&& decision.spec_candidate_bound()
        &&& decision.spec_requirements_complete()
        &&& decision.spec_criteria_complete()
        &&& decision.spec_inventories_complete()
        &&& decision.spec_independent_review_complete()
        &&& decision.spec_blockers_clear()
        &&& decision.spec_evidence_complete()
        &&& decision.spec_unmet_conditions().len() == 0
    }
}

/// Evaluates H0 security readiness for one exact integrated candidate.
///
/// Conditions are emitted in candidate-binding, R-SEC, acceptance-criterion, inventory, external
/// review, finding, and evidence-manifest order. The function performs no effects and its result
/// does not confer release authority.
#[must_use]
pub fn evaluate_security_readiness(
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
) -> (decision: SecurityDecision)
    ensures
        ready_evaluation_contract(&decision, candidate, evidence),
        decision.spec_is_ready() ==> evidence.spec_all_current(candidate),
        decision.spec_is_ready() ==> decision.spec_checks_complete(),
        decision.spec_is_ready() ==> decision.spec_candidate_bound(),
        decision.spec_is_ready() ==> decision.spec_requirements_complete(),
        decision.spec_is_ready() ==> decision.spec_criteria_complete(),
        decision.spec_is_ready() ==> decision.spec_inventories_complete(),
        decision.spec_is_ready() ==> decision.spec_independent_review_complete(),
        decision.spec_is_ready() ==> decision.spec_blockers_clear(),
        decision.spec_is_ready() ==> decision.spec_evidence_complete(),
        decision.spec_is_ready() ==> decision.spec_unmet_conditions().len() == 0,
{
    let mut unmet = Vec::<UnmetSecurityCondition>::new();
    let candidate_bound = binding::evaluate(candidate, evidence, &mut unmet);
    let requirements_complete = controls::evaluate_requirements(candidate, evidence, &mut unmet);
    let criteria_complete = controls::evaluate_criteria(candidate, evidence, &mut unmet);
    let inventories_complete = inventories::evaluate(candidate, evidence, &mut unmet);
    let (independent_review_complete, blockers_clear) =
        review::evaluate(candidate, evidence, &mut unmet);
    let evidence_complete = artifacts::evaluate(candidate, evidence, &mut unmet);
    let checks = SecurityChecks::new(
        CheckResult::from_bool(candidate_bound),
        CheckResult::from_bool(requirements_complete),
        CheckResult::from_bool(criteria_complete),
        CheckResult::from_bool(inventories_complete),
        CheckResult::from_bool(independent_review_complete),
        CheckResult::from_bool(blockers_clear),
        CheckResult::from_bool(evidence_complete),
    );
    let decision = SecurityDecision::from_evaluation(unmet, checks);
    proof {
        reveal(ready_evaluation_contract);
        if decision.spec_is_ready() {
            decision.ready_has_complete_checks();
            decision.ready_has_all_security_obligations();
        }
    }
    decision
}

} // verus!
