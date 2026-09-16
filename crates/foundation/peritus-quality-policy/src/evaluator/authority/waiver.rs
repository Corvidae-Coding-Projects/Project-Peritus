//! Policy authority and approval refinement for a selected finding and waiver.

use super::lookup::{self, current_approval, current_waiver};
use crate::evaluator::requirements;
#[cfg(verus_only)]
use crate::model::authority::*;
use crate::{
    AcceptanceEvidence, ApprovalOutcome, ApprovalSubject, FindingDisposition, FindingObservation,
    InvalidWaiverReason, WaiverObservation,
};
use peritus_spec::{AcceptanceContract, WaiverPolicy};
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

pub(super) fn waiver_failure(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    finding: &FindingObservation,
    waiver: &WaiverObservation,
) -> (result: Option<InvalidWaiverReason>)
    ensures
        result == waiver_failure_reason(contract, requested, evidence, *finding, *waiver),
        result.is_none() == waiver_parameters_allowed(contract, requested, evidence, *finding, *waiver),
{
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
    if !crate::revision::evidence_requirement_matches(waiver.evidence_requirement_id(), requirement) {
        return Some(InvalidWaiverReason::WrongEvidenceRequirement);
    }
    if !requirements::has_current(evidence, requirement, requested) {
        return Some(InvalidWaiverReason::MissingEvidence);
    }
    let Some(approval_index) = current_approval(evidence, waiver.approval_request_id(), requested) else {
        return Some(InvalidWaiverReason::MissingApproval);
    };
    let approval = &evidence.approvals()[approval_index];
    proof {
        assert(matching_waiver_approval(evidence, waiver.spec_approval_request_id(), requested, finding.spec_finding_id(), authority)
            == approval_has_subject_authority(*approval, finding.spec_finding_id(), authority));
        assert(approved_waiver_approval(evidence, waiver.spec_approval_request_id(), requested, finding.spec_finding_id(), authority)
            == (approval_has_subject_authority(*approval, finding.spec_finding_id(), authority) && approval.spec_outcome() == ApprovalOutcome::Approved));
    }
    if !lookup::subject_equal(approval.subject(), ApprovalSubject::FindingWaiver(finding.finding_id()))
        || !crate::revision::digest_matches(approval.authority().digest(), authority.digest())
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
) -> (valid: bool)
    ensures valid == current_finding_waived(contract, requested, evidence, *finding),
{
    #[allow(
        clippy::option_if_let_else,
        reason = "explicit Option branches remain directly supported and auditable in Verus"
    )]
    if let Some(index) = current_waiver(evidence, finding.finding_id(), requested) {
        let waiver = &evidence.waivers()[index];
        let valid = waiver_failure(contract, requested, evidence, finding, waiver).is_none();
        assert forall |other: int| 0 <= other < evidence.spec_waivers().len()
            && waiver_authorized(contract, requested, evidence, *finding, #[trigger] evidence.spec_waivers()[other])
            implies other == index by {
            assert(current_waiver_at(evidence.spec_waivers(), finding.spec_finding_id(), requested, other));
        }
        assert(current_finding_waived(contract, requested, evidence, *finding)
            == waiver_parameters_allowed(contract, requested, evidence, *finding, *waiver));
        valid
    } else {
        assert forall |other: int| 0 <= other < evidence.spec_waivers().len()
            implies !waiver_authorized(contract, requested, evidence, *finding, #[trigger] evidence.spec_waivers()[other]) by {
            assert(!current_waiver_at(evidence.spec_waivers(), finding.spec_finding_id(), requested, other));
        }
        false
    }
}

/// Classifies a supplied current waiver against its unique current finding.
pub(super) fn supplied_failure(
    contract: &AcceptanceContract, requested: RevisionTuple,
    evidence: &AcceptanceEvidence, waiver: &WaiverObservation,
) -> (result: Option<InvalidWaiverReason>)
    ensures crate::model::revision_fresh(waiver.spec_revision(), requested) ==>
        (result.is_none() == supplied_waiver_valid(contract, requested, evidence, *waiver)),
{
    match lookup::current_finding(evidence, waiver.finding_id(), requested) {
        None => Some(InvalidWaiverReason::UnknownFinding),
        Some((review, index)) => {
            let finding = &evidence.reviews()[review].findings()[index];
            let result = waiver_failure(contract, requested, evidence, finding, waiver);
            assert(supplied_waiver_valid(contract, requested, evidence, *waiver)
                == waiver_authorized(contract, requested, evidence, *finding, *waiver));
            result
        },
    }
}

} // verus!
