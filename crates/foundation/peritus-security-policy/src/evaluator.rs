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

/// Exact deterministic admission predicate over every supplied H0 observation.
pub open spec fn security_evaluation_inputs_complete(
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
) -> bool {
    evidence.spec_all_current(candidate)
        && controls::requirements_complete(evidence, candidate)
        && controls::criteria_complete(evidence, candidate)
        && inventories::inventories_complete(evidence, candidate)
        && review::independent_review_complete(evidence, candidate)
        && review::release_blockers_clear(evidence, candidate)
        && artifacts::evidence_complete(evidence, candidate)
}

/// Formal contract established by every ready result.
pub open spec fn ready_evaluation_contract(
    decision: &SecurityDecision,
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
) -> bool {
    &&& decision.spec_candidate_bound() == evidence.spec_all_current(candidate)
    &&& decision.spec_requirements_complete()
        == controls::requirements_complete(evidence, candidate)
    &&& decision.spec_criteria_complete() == controls::criteria_complete(evidence, candidate)
    &&& decision.spec_inventories_complete()
        == inventories::inventories_complete(evidence, candidate)
    &&& decision.spec_independent_review_complete()
        == review::independent_review_complete(evidence, candidate)
    &&& decision.spec_blockers_clear()
        == review::release_blockers_clear(evidence, candidate)
    &&& decision.spec_evidence_complete() == artifacts::evidence_complete(evidence, candidate)
    &&& (decision.spec_verdict() == crate::SecurityVerdict::Ready)
        == security_evaluation_inputs_complete(candidate, evidence)
    &&& decision.spec_is_ready() == security_evaluation_inputs_complete(candidate, evidence)
    &&& (decision.spec_is_ready() ==> {
        &&& evidence.spec_all_current(candidate)
        &&& decision.spec_checks_complete()
        &&& decision.spec_candidate_bound()
        &&& decision.spec_requirements_complete()
        &&& decision.spec_criteria_complete()
        &&& controls::requirements_complete(evidence, candidate)
        &&& controls::criteria_complete(evidence, candidate)
        &&& decision.spec_inventories_complete()
        &&& decision.spec_independent_review_complete()
        &&& decision.spec_blockers_clear()
        &&& decision.spec_evidence_complete()
        &&& decision.spec_unmet_conditions().len() == 0
    })
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
        decision.spec_candidate_bound() == evidence.spec_all_current(candidate),
        decision.spec_requirements_complete()
            == controls::requirements_complete(evidence, candidate),
        decision.spec_criteria_complete() == controls::criteria_complete(evidence, candidate),
        decision.spec_inventories_complete()
            == inventories::inventories_complete(evidence, candidate),
        decision.spec_independent_review_complete()
            == review::independent_review_complete(evidence, candidate),
        decision.spec_blockers_clear()
            == review::release_blockers_clear(evidence, candidate),
        decision.spec_evidence_complete() == artifacts::evidence_complete(evidence, candidate),
        (decision.spec_verdict() == crate::SecurityVerdict::Ready)
            == security_evaluation_inputs_complete(candidate, evidence),
        decision.spec_is_ready() == security_evaluation_inputs_complete(candidate, evidence),
        decision.spec_is_ready() ==> evidence.spec_all_current(candidate),
        decision.spec_is_ready() ==> decision.spec_checks_complete(),
        decision.spec_is_ready() ==> decision.spec_candidate_bound(),
        decision.spec_is_ready() ==> decision.spec_requirements_complete(),
        decision.spec_is_ready() ==> decision.spec_criteria_complete(),
        decision.spec_is_ready() ==> controls::requirements_complete(evidence, candidate),
        decision.spec_is_ready() ==> controls::criteria_complete(evidence, candidate),
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
    proof {
        if security_evaluation_inputs_complete(candidate, evidence) {
            assert(candidate_bound);
            assert(requirements_complete);
            assert(criteria_complete);
            assert(inventories_complete);
            assert(independent_review_complete);
            assert(blockers_clear);
            assert(evidence_complete);
            assert(unmet@.len() == 0);
        }
    }
    let ghost evaluated_unmet = unmet@;
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
        reveal(security_evaluation_inputs_complete);
        assert(decision.spec_candidate_bound() == candidate_bound);
        assert(decision.spec_requirements_complete() == requirements_complete);
        assert(decision.spec_criteria_complete() == criteria_complete);
        assert(decision.spec_inventories_complete() == inventories_complete);
        assert(decision.spec_independent_review_complete() == independent_review_complete);
        assert(decision.spec_blockers_clear() == blockers_clear);
        assert(decision.spec_evidence_complete() == evidence_complete);
        if security_evaluation_inputs_complete(candidate, evidence) {
            assert(evaluated_unmet.len() == 0);
            assert(checks.spec_complete());
            assert(decision.spec_is_ready());
        }
        if decision.spec_is_ready() {
            decision.ready_has_complete_checks();
            decision.ready_has_all_security_obligations();
            reveal(crate::model::security_ready);
            assert(checks.spec_complete());
            assert(requirements_complete);
            assert(criteria_complete);
            assert(controls::requirements_complete(evidence, candidate));
            assert(controls::criteria_complete(evidence, candidate));
        }
    }
    decision
}

} // verus!
