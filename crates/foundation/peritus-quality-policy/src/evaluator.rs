//! Total effect-free acceptance evaluation.

mod authority;
mod freshness;
mod gates;
mod requirements;
mod reviews;

use crate::{AcceptanceDecision, AcceptanceEvidence, UnmetCondition};
use crate::decision::{AcceptanceChecks, CheckResult};
use peritus_spec::AcceptanceContract;
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

/// Formal contract established by every acceptable result of [`evaluate_acceptance`].
pub open spec fn accepted_evaluation_contract(
    decision: &AcceptanceDecision,
    contract: &AcceptanceContract,
    requested_revision: RevisionTuple,
    evidence: &AcceptanceEvidence,
) -> bool {
    decision.spec_is_acceptable() ==> {
        &&& evidence.spec_all_current(requested_revision)
        &&& peritus_spec::acceptance_ids_match(
            requested_revision.spec_acceptance_spec_id(),
            contract.spec_id(),
        )
        &&& crate::model::passing_gate_attempts_within_limit(
            evidence.spec_gates(),
            requested_revision,
            decision.spec_gate_attempt_limit(),
        )
        &&& crate::model::review_cycles_within_limit(
            evidence.spec_reviews(),
            requested_revision,
            decision.spec_review_cycle_limit(),
        )
        &&& decision.spec_unmet_conditions().len() == 0
        &&& decision.spec_checks_complete()
    }
}

/// Evaluates one immutable contract against evidence for one exact revision tuple.
///
/// The result is deterministic: conditions are emitted in binding, freshness, gate, evidence,
/// review, blocker/waiver, and final-approval order. This function performs no I/O and grants no
/// lifecycle authority by itself.
#[must_use]
pub fn evaluate_acceptance(
    contract: &AcceptanceContract,
    requested_revision: RevisionTuple,
    evidence: &AcceptanceEvidence,
) -> (decision: AcceptanceDecision)
    ensures
        accepted_evaluation_contract(
            &decision,
            contract,
            requested_revision,
            evidence,
        ),
        decision.spec_is_acceptable() ==> evidence.spec_all_current(requested_revision),
        decision.spec_is_acceptable() ==> peritus_spec::acceptance_ids_match(
            requested_revision.spec_acceptance_spec_id(), contract.spec_id()),
        decision.spec_is_acceptable() ==> crate::model::passing_gate_attempts_within_limit(
            evidence.spec_gates(),
            requested_revision,
            decision.spec_gate_attempt_limit(),
        ),
        decision.spec_is_acceptable() ==> crate::model::review_cycles_within_limit(
            evidence.spec_reviews(),
            requested_revision,
            decision.spec_review_cycle_limit(),
        ),
        decision.spec_is_acceptable() ==> decision.spec_unmet_conditions().len() == 0,
{
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
    let gates_complete = gates::evaluate(
        contract,
        requested_revision,
        evidence,
        maximum_gate_attempts,
        &mut unmet,
    );
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
    let approvals_expected =
        authority::evaluate_unexpected_approvals(contract, requested_revision, evidence, &mut unmet);

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
    proof {
        reveal(accepted_evaluation_contract);
        if decision.spec_is_acceptable() {
            decision.accepted_has_complete_checks();
        }
    }
    decision
}

} // verus!
