//! Total effect-free security-readiness evaluation.

mod artifacts;
mod binding;
mod controls;
mod inventories;
mod review;

use crate::decision::{CheckResult, SecurityChecks};
use crate::{IntegratedCandidate, SecurityDecision, SecurityEvidence, UnmetSecurityCondition};

#[doc = " Evaluates H0 security readiness for one exact integrated candidate."]
#[doc = ""]
#[doc = " Conditions are emitted in candidate-binding, R-SEC, acceptance-criterion, inventory, external"]
#[doc = " review, finding, and evidence-manifest order. The function performs no effects and its result"]
#[doc = " does not confer release authority."]
#[must_use]
pub fn evaluate_security_readiness(
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
) -> SecurityDecision {
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
    decision
}
