//! Blocker, waiver, and final human-approval evaluation.

mod blockers;
pub(super) mod diagnostics;
mod lookup;
mod supplied;
mod waiver;

use crate::{AcceptanceEvidence, ApprovalOutcome, ApprovalSubject, UnmetCondition};

use peritus_spec::{AcceptanceContract, HumanApprovalPolicy};
use peritus_types::RevisionTuple;

pub(super) fn evaluate_waivers(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    unmet: &mut Vec<UnmetCondition>,
) -> bool {
    let supplied_valid = supplied::evaluate(contract, requested, evidence, unmet);
    let blockers_resolved = blockers::evaluate(contract, requested, evidence, unmet);
    supplied_valid && blockers_resolved
}
#[allow(
    clippy::option_if_let_else,
    reason = "explicit Option branches preserve the audited approval-diagnostic precedence in Verus"
)]
pub(super) fn evaluate_final_approval(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    unmet: &mut Vec<UnmetCondition>,
) -> bool {
    let failure = match contract.approval_policy() {
        HumanApprovalPolicy::NotRequired => None,
        HumanApprovalPolicy::Required(required_authority) => {
            match current_acceptance_approval(evidence, requested) {
                None => Some(UnmetCondition::MissingHumanApproval),
                Some(index) => {
                    let approval = &evidence.approvals()[index];
                    if crate::revision::digest_matches(
                        approval.authority().digest(),
                        required_authority.digest(),
                    ) {
                        match approval.outcome() {
                            ApprovalOutcome::Denied => Some(UnmetCondition::HumanApprovalDenied),
                            ApprovalOutcome::Approved => None,
                        }
                    } else {
                        Some(UnmetCondition::WrongHumanApprovalAuthority)
                    }
                }
            }
        }
    };
    match failure {
        None => true,
        Some(condition) => {
            unmet.push(condition);
            false
        }
    }
}
fn current_acceptance_approval(
    evidence: &AcceptanceEvidence,
    requested: RevisionTuple,
) -> Option<usize> {
    let mut index = 0;
    while index < evidence.approvals().len() {
        let approval = &evidence.approvals()[index];
        if crate::revision::revision_matches(approval.revision(), requested)
            && lookup::subject_equal(approval.subject(), ApprovalSubject::Acceptance)
        {
            return Some(index);
        }
        index += 1;
    }
    None
}
fn waiver_approval_expected(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    approval: &crate::ApprovalObservation,
    finding: peritus_types::FindingId,
) -> bool {
    let mut found = false;
    let mut waiver_index = 0;
    while waiver_index < evidence.waivers().len() {
        let waiver = &evidence.waivers()[waiver_index];
        if crate::revision::revision_matches(waiver.revision(), requested)
            && lookup::finding_equal(waiver.finding_id(), finding)
            && lookup::request_equal(waiver.approval_request_id(), approval.request_id())
            && waiver::supplied_failure(contract, requested, evidence, waiver).is_none()
        {
            found = true;
        }

        waiver_index += 1;
    }
    found
}
fn approval_expected(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    approval: &crate::ApprovalObservation,
) -> bool {
    match approval.subject() {
        ApprovalSubject::Acceptance => contract.approval_policy().is_required(),
        ApprovalSubject::FindingWaiver(finding) => {
            waiver_approval_expected(contract, requested, evidence, approval, finding)
        }
    }
}
pub(super) fn evaluate_unexpected_approvals(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    unmet: &mut Vec<UnmetCondition>,
) -> bool {
    let mut complete = true;
    let mut index = 0;
    while index < evidence.approvals().len() {
        let approval = &evidence.approvals()[index];
        if crate::revision::revision_matches(approval.revision(), requested)
            && !approval_expected(contract, requested, evidence, approval)
        {
            complete = false;
            unmet.push(UnmetCondition::UnexpectedApproval(approval.actor_id()));
        }
        index += 1;
    }
    complete
}
