//! Policy authority and approval refinement for a selected finding and waiver.

use super::lookup::{self, current_approval, current_waiver};
use crate::evaluator::requirements;

use crate::{
    AcceptanceEvidence, ApprovalOutcome, ApprovalSubject, FindingDisposition, FindingObservation,
    InvalidWaiverReason, WaiverObservation,
};
use peritus_spec::{AcceptanceContract, WaiverPolicy};
use peritus_types::RevisionTuple;

pub(super) fn waiver_failure(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    finding: &FindingObservation,
    waiver: &WaiverObservation,
) -> Option<InvalidWaiverReason> {
    match finding.disposition() {
        FindingDisposition::Resolved { .. } => {
            return Some(InvalidWaiverReason::AlreadyResolved);
        }
        FindingDisposition::Open => return Some(InvalidWaiverReason::NotRequested),
        FindingDisposition::WaiverRequested => {}
    }
    let (authority, requirement) = match contract.waiver_policy() {
        WaiverPolicy::Forbidden => return Some(InvalidWaiverReason::Forbidden),
        WaiverPolicy::Allowed { authority, evidence } => (authority, evidence),
    };
    if !crate::revision::digest_matches(waiver.authority().digest(), authority.digest()) {
        return Some(InvalidWaiverReason::WrongAuthority);
    }
    if !crate::revision::evidence_requirement_matches(waiver.evidence_requirement_id(), requirement)
    {
        return Some(InvalidWaiverReason::WrongEvidenceRequirement);
    }
    if !requirements::has_current(evidence, requirement, requested) {
        return Some(InvalidWaiverReason::MissingEvidence);
    }
    let Some(approval_index) = current_approval(evidence, waiver.approval_request_id(), requested)
    else {
        return Some(InvalidWaiverReason::MissingApproval);
    };
    let approval = &evidence.approvals()[approval_index];
    if !lookup::subject_equal(
        approval.subject(),
        ApprovalSubject::FindingWaiver(finding.finding_id()),
    ) || !crate::revision::digest_matches(approval.authority().digest(), authority.digest())
    {
        return Some(InvalidWaiverReason::MissingApproval);
    }
    match approval.outcome() {
        ApprovalOutcome::Approved => None,
        ApprovalOutcome::Denied => Some(InvalidWaiverReason::ApprovalDenied),
    }
}
pub(super) fn current_waiver_is_valid(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    finding: &FindingObservation,
) -> bool {
    #[allow(
        clippy::option_if_let_else,
        reason = "explicit Option branches remain directly supported and auditable in Verus"
    )]
    if let Some(index) = current_waiver(evidence, finding.finding_id(), requested) {
        let waiver = &evidence.waivers()[index];
        let valid = waiver_failure(contract, requested, evidence, finding, waiver).is_none();
        valid
    } else {
        false
    }
}
#[doc = " Classifies a supplied current waiver against its unique current finding."]
pub(super) fn supplied_failure(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    waiver: &WaiverObservation,
) -> Option<InvalidWaiverReason> {
    match lookup::current_finding(evidence, waiver.finding_id(), requested) {
        None => Some(InvalidWaiverReason::UnknownFinding),
        Some((review, index)) => {
            let finding = &evidence.reviews()[review].findings()[index];
            let result = waiver_failure(contract, requested, evidence, finding, waiver);
            result
        }
    }
}
