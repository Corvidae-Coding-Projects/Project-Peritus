//! Input-defined waiver authority, blocker resolution, and diagnostic coverage.

#[cfg(verus_only)]
use crate::{
    AcceptanceEvidence, ApprovalObservation, ApprovalOutcome, ApprovalSubject, FindingDisposition,
    FindingObservation, InvalidWaiverReason, ReviewObservation, UnmetCondition, WaiverObservation,
};
#[cfg(verus_only)]
use peritus_spec::{AcceptanceContract, ContentReference, FindingSeverity, WaiverPolicy};
#[cfg(verus_only)]
use peritus_types::{ApprovalRequestId, FindingId, RevisionTuple};
use vstd::prelude::*;

verus! {

/// Compares every byte of two finding identities.
pub open spec fn finding_ids_match(left: FindingId, right: FindingId) -> bool {
    left.spec_bytes()@ == right.spec_bytes()@
}

pub open spec fn request_ids_match(left: ApprovalRequestId, right: ApprovalRequestId) -> bool {
    left.spec_bytes()@ == right.spec_bytes()@
}

pub open spec fn authority_matches(left: ContentReference, right: ContentReference) -> bool {
    crate::model::digests_match(left.spec_digest(), right.spec_digest())
}

pub open spec fn current_approval_at(values: Seq<ApprovalObservation>, request: ApprovalRequestId, revision: RevisionTuple, index: int) -> bool {
    0 <= index < values.len() && request_ids_match(values[index].spec_request_id(), request)
        && crate::model::revision_fresh(values[index].spec_revision(), revision)
}

/// Selects a supplied waiver with the requested finding identity and current revision.
pub open spec fn current_waiver_at(values: Seq<WaiverObservation>, finding: FindingId, revision: RevisionTuple, index: int) -> bool {
    0 <= index < values.len() && finding_ids_match(values[index].spec_finding_id(), finding)
        && crate::model::revision_fresh(values[index].spec_revision(), revision)
}

/// Selects a finding by identity within a supplied review at the current revision.
pub open spec fn current_finding_at(values: Seq<ReviewObservation>, finding: FindingId, revision: RevisionTuple, review: int, index: int) -> bool {
    0 <= review < values.len() && 0 <= index < values[review].spec_findings().len()
        && crate::model::revision_fresh(values[review].spec_revision(), revision)
        && finding_ids_match(values[review].spec_findings()[index].spec_finding_id(), finding)
}

pub open spec fn approval_has_subject_authority(approval: ApprovalObservation, finding: FindingId, authority: ContentReference) -> bool {
    crate::canonical::collections::subjects_match(approval.spec_subject(), ApprovalSubject::FindingWaiver(finding))
        && authority_matches(approval.spec_authority(), authority)
}

pub open spec fn matching_waiver_approval(evidence: &AcceptanceEvidence, request: ApprovalRequestId, revision: RevisionTuple, finding: FindingId, authority: ContentReference) -> bool {
    exists |index: int| #[trigger] current_approval_at(evidence.spec_approvals(), request, revision, index)
        && approval_has_subject_authority(evidence.spec_approvals()[index], finding, authority)
}

pub open spec fn approved_waiver_approval(evidence: &AcceptanceEvidence, request: ApprovalRequestId, revision: RevisionTuple, finding: FindingId, authority: ContentReference) -> bool {
    exists |index: int| #[trigger] current_approval_at(evidence.spec_approvals(), request, revision, index)
        && approval_has_subject_authority(evidence.spec_approvals()[index], finding, authority)
        && evidence.spec_approvals()[index].spec_outcome() == ApprovalOutcome::Approved
}

/// Authority checks performed for the supplied finding and waiver values.
/// Current finding membership and matching waiver identity are separate lookup obligations.
pub open spec fn waiver_parameters_allowed(contract: &AcceptanceContract, revision: RevisionTuple, evidence: &AcceptanceEvidence, finding: FindingObservation, waiver: WaiverObservation) -> bool {
    finding.spec_disposition() == FindingDisposition::WaiverRequested
        && match contract.spec_waiver_policy() {
            WaiverPolicy::Forbidden => false,
            WaiverPolicy::Allowed { authority, evidence: requirement } => {
                authority_matches(waiver.spec_authority(), authority)
                    && crate::model::evidence_requirement_matches(waiver.spec_evidence_requirement_id(), requirement)
                    && crate::model::current_artifact_present(evidence.spec_evidence(), requirement, revision)
                    && approved_waiver_approval(evidence, waiver.spec_approval_request_id(), revision, finding.spec_finding_id(), authority)
            },
        }
}

/// Exact diagnostic priority, defined from supplied policy and observations.
pub open spec fn waiver_failure_reason(contract: &AcceptanceContract, revision: RevisionTuple, evidence: &AcceptanceEvidence, finding: FindingObservation, waiver: WaiverObservation) -> Option<InvalidWaiverReason> {
    match finding.spec_disposition() {
        FindingDisposition::Resolved { .. } => Some(InvalidWaiverReason::AlreadyResolved),
        FindingDisposition::Open => Some(InvalidWaiverReason::NotRequested),
        FindingDisposition::WaiverRequested => match contract.spec_waiver_policy() {
            WaiverPolicy::Forbidden => Some(InvalidWaiverReason::Forbidden),
            WaiverPolicy::Allowed { authority, evidence: requirement } => {
                if !authority_matches(waiver.spec_authority(), authority) { Some(InvalidWaiverReason::WrongAuthority) }
                else if !crate::model::evidence_requirement_matches(waiver.spec_evidence_requirement_id(), requirement) { Some(InvalidWaiverReason::WrongEvidenceRequirement) }
                else if !crate::model::current_artifact_present(evidence.spec_evidence(), requirement, revision) { Some(InvalidWaiverReason::MissingEvidence) }
                else if !matching_waiver_approval(evidence, waiver.spec_approval_request_id(), revision, finding.spec_finding_id(), authority) { Some(InvalidWaiverReason::MissingApproval) }
                else if !approved_waiver_approval(evidence, waiver.spec_approval_request_id(), revision, finding.spec_finding_id(), authority) { Some(InvalidWaiverReason::ApprovalDenied) }
                else { None }
            },
        },
    }
}

/// A current waiver matches the finding and satisfies all declared authority requirements.
pub open spec fn waiver_authorized(contract: &AcceptanceContract, revision: RevisionTuple, evidence: &AcceptanceEvidence, finding: FindingObservation, waiver: WaiverObservation) -> bool {
    crate::model::revision_fresh(waiver.spec_revision(), revision)
        && finding_ids_match(waiver.spec_finding_id(), finding.spec_finding_id())
        && waiver_parameters_allowed(contract, revision, evidence, finding, waiver)
}

/// The supplied waiver is authorized for a unique finding in a current review.
pub open spec fn supplied_waiver_valid(contract: &AcceptanceContract, revision: RevisionTuple, evidence: &AcceptanceEvidence, waiver: WaiverObservation) -> bool {
    exists |review: int, index: int| #[trigger] current_finding_at(evidence.spec_reviews(), waiver.spec_finding_id(), revision, review, index)
        && waiver_authorized(contract, revision, evidence, evidence.spec_reviews()[review].spec_findings()[index], waiver)
}

/// Every supplied current waiver satisfies its finding and authority requirements.
pub open spec fn supplied_current_waivers_valid(contract: &AcceptanceContract, revision: RevisionTuple, evidence: &AcceptanceEvidence) -> bool {
    forall |index: int| 0 <= index < evidence.spec_waivers().len()
        && crate::model::revision_fresh(#[trigger] evidence.spec_waivers()[index].spec_revision(), revision)
        ==> supplied_waiver_valid(contract, revision, evidence, evidence.spec_waivers()[index])
}

pub open spec fn current_finding_waived(contract: &AcceptanceContract, revision: RevisionTuple, evidence: &AcceptanceEvidence, finding: FindingObservation) -> bool {
    exists |index: int| 0 <= index < evidence.spec_waivers().len()
        && waiver_authorized(contract, revision, evidence, finding, #[trigger] evidence.spec_waivers()[index])
}

pub open spec fn severity_rank(severity: FindingSeverity) -> nat {
    match severity { FindingSeverity::Advisory => 0, FindingSeverity::Low => 1, FindingSeverity::Medium => 2, FindingSeverity::High => 3, FindingSeverity::Critical => 4 }
}

pub open spec fn finding_resolved_or_waived(contract: &AcceptanceContract, revision: RevisionTuple, evidence: &AcceptanceEvidence, finding: FindingObservation) -> bool {
    match finding.spec_disposition() {
        FindingDisposition::Resolved { revision: resolution, .. } => crate::model::revision_fresh(resolution, revision),
        _ => current_finding_waived(contract, revision, evidence, finding),
    }
}

/// Every current finding at or above the policy threshold is freshly resolved or authorized for waiver.
pub open spec fn blocking_findings_resolved(contract: &AcceptanceContract, revision: RevisionTuple, evidence: &AcceptanceEvidence) -> bool {
    forall |review: int, index: int| 0 <= review < evidence.spec_reviews().len()
        && 0 <= index < evidence.spec_reviews()[review].spec_findings().len()
        && crate::model::revision_fresh(evidence.spec_reviews()[review].spec_revision(), revision)
        && severity_rank(#[trigger] evidence.spec_reviews()[review].spec_findings()[index].spec_severity()) >= severity_rank(contract.spec_review_policy().spec_blocking_severity())
        ==> finding_resolved_or_waived(contract, revision, evidence, evidence.spec_reviews()[review].spec_findings()[index])
}

/// All supplied current waivers are valid and all current blockers are resolved or waived.
pub open spec fn waiver_phase_complete(contract: &AcceptanceContract, revision: RevisionTuple, evidence: &AcceptanceEvidence) -> bool {
    supplied_current_waivers_valid(contract, revision, evidence) && blocking_findings_resolved(contract, revision, evidence)
}

/// At least one invalid-waiver diagnostic identifies the complete finding identity.
pub open spec fn invalid_waiver_reported(conditions: Seq<UnmetCondition>, finding: FindingId) -> bool {
    exists |index: int| 0 <= index < conditions.len() && match #[trigger] conditions[index] {
        UnmetCondition::InvalidWaiver { finding_id, .. } => finding_ids_match(finding_id, finding),
        _ => false,
    }
}

/// Every unusable supplied current waiver is diagnosed, even on an unacceptable decision.
pub open spec fn invalid_waivers_reported(contract: &AcceptanceContract, revision: RevisionTuple, evidence: &AcceptanceEvidence, conditions: Seq<UnmetCondition>) -> bool {
    forall |index: int| 0 <= index < evidence.spec_waivers().len()
        && crate::model::revision_fresh(#[trigger] evidence.spec_waivers()[index].spec_revision(), revision)
        && !supplied_waiver_valid(contract, revision, evidence, evidence.spec_waivers()[index])
        ==> invalid_waiver_reported(conditions, evidence.spec_waivers()[index].spec_finding_id())
}

} // verus!
