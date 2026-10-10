//! Total effect-free acceptance evaluation.

mod authority;
mod freshness;
mod gates;
mod requirements;
mod reviews;

use crate::decision::{AcceptanceChecks, CheckResult};
use crate::{AcceptanceDecision, AcceptanceEvidence, UnmetCondition};
use peritus_spec::AcceptanceContract;
use peritus_types::RevisionTuple;

#[doc = " Evaluates one immutable contract against evidence for one exact revision tuple."]
#[doc = ""]
#[doc = " The result is deterministic: conditions are emitted in binding, freshness, gate, evidence,"]
#[doc = " review, blocker/waiver, and final-approval order. This function performs no I/O and grants no"]
#[doc = " lifecycle authority by itself."]
#[must_use]
pub fn evaluate_acceptance(
    contract: &AcceptanceContract,
    requested_revision: RevisionTuple,
    evidence: &AcceptanceEvidence,
) -> AcceptanceDecision {
    let mut unmet = Vec::<UnmetCondition>::new();
    let contract_bound = crate::revision::acceptance_id_matches(
        contract.id(),
        requested_revision.acceptance_spec_id(),
    );
    if !contract_bound {
        unmet.push(UnmetCondition::ContractRevisionMismatch);
    }
    let observations_fresh = freshness::evaluate(requested_revision, evidence, &mut unmet);
    let maximum_gate_attempts = contract.completion_policy().max_gate_attempts();
    let maximum_review_cycles = contract.completion_policy().max_review_cycles();
    let gates_complete =
        gates::evaluate(contract, requested_revision, evidence, maximum_gate_attempts, &mut unmet);
    let evidence_complete =
        requirements::evaluate(contract, requested_revision, evidence, &mut unmet);
    let reviews_complete = reviews::evaluate(
        contract,
        requested_revision,
        evidence,
        maximum_review_cycles,
        &mut unmet,
    );
    let blockers_complete =
        authority::evaluate_waivers(contract, requested_revision, evidence, &mut unmet);
    let final_approval_complete =
        authority::evaluate_final_approval(contract, requested_revision, evidence, &mut unmet);
    let approvals_expected = authority::evaluate_unexpected_approvals(
        contract,
        requested_revision,
        evidence,
        &mut unmet,
    );
    let checks = AcceptanceChecks::new(
        CheckResult::from_bool(contract_bound),
        CheckResult::from_bool(observations_fresh),
        CheckResult::from_bool(gates_complete),
        CheckResult::from_bool(evidence_complete),
        CheckResult::from_bool(reviews_complete),
        CheckResult::from_bool(blockers_complete),
        CheckResult::from_bool(final_approval_complete && approvals_expected),
    );
    let decision = AcceptanceDecision::from_evaluation(
        unmet,
        checks,
        maximum_gate_attempts,
        maximum_review_cycles,
    );
    decision
}
