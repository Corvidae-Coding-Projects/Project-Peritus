//! Blocker, waiver, and final human-approval evaluation.

use super::requirements;
use crate::{
    AcceptanceEvidence, ApprovalObservation, ApprovalOutcome, ApprovalSubject, FindingDisposition,
    FindingObservation, InvalidWaiverReason, UnmetCondition, WaiverObservation,
};
use peritus_spec::{AcceptanceContract, HumanApprovalPolicy, WaiverPolicy};
use peritus_types::{ApprovalRequestId, FindingId, RevisionTuple};
use vstd::prelude::*;

verus! {

fn current_waiver(
    evidence: &AcceptanceEvidence,
    finding_id: FindingId,
    requested: RevisionTuple,
) -> Option<&WaiverObservation> {
    let mut index = 0;
    while index < evidence.waivers().len()
        invariant 0 <= index <= evidence.spec_waivers().len(),
        decreases evidence.spec_waivers().len() - index,
    {
        let waiver = &evidence.waivers()[index];
        if waiver.finding_id() == finding_id && waiver.revision() == requested {
            return Some(waiver);
        }
        index += 1;
    }
    None
}

fn current_approval(
    evidence: &AcceptanceEvidence,
    request_id: ApprovalRequestId,
    requested: RevisionTuple,
) -> Option<&ApprovalObservation> {
    let mut index = 0;
    while index < evidence.approvals().len()
        invariant 0 <= index <= evidence.spec_approvals().len(),
        decreases evidence.spec_approvals().len() - index,
    {
        let approval = &evidence.approvals()[index];
        if approval.request_id() == request_id && approval.revision() == requested {
            return Some(approval);
        }
        index += 1;
    }
    None
}

fn current_finding(
    evidence: &AcceptanceEvidence,
    finding_id: FindingId,
    requested: RevisionTuple,
) -> Option<&FindingObservation> {
    let mut review_index = 0;
    while review_index < evidence.reviews().len()
        invariant 0 <= review_index <= evidence.spec_reviews().len(),
        decreases evidence.spec_reviews().len() - review_index,
    {
        if evidence.reviews()[review_index].revision() == requested {
            let findings = evidence.reviews()[review_index].findings();
            let mut finding_index = 0;
            while finding_index < findings.len()
                invariant 0 <= finding_index <= findings.len(),
                decreases findings.len() - finding_index,
            {
                if findings[finding_index].finding_id() == finding_id {
                    return Some(&findings[finding_index]);
                }
                finding_index += 1;
            }
        }
        review_index += 1;
    }
    None
}

fn waiver_failure(
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
    if waiver.authority() != authority {
        return Some(InvalidWaiverReason::WrongAuthority);
    }
    if waiver.evidence_requirement_id() != requirement {
        return Some(InvalidWaiverReason::WrongEvidenceRequirement);
    }
    if !requirements::has_current(evidence, requirement, requested) {
        return Some(InvalidWaiverReason::MissingEvidence);
    }
    let Some(approval) = current_approval(evidence, waiver.approval_request_id(), requested) else {
        return Some(InvalidWaiverReason::MissingApproval);
    };
    if approval.subject() != ApprovalSubject::FindingWaiver(finding.finding_id())
        || approval.authority() != authority
    {
        return Some(InvalidWaiverReason::MissingApproval);
    }
    match approval.outcome() {
        ApprovalOutcome::Approved => None,
        ApprovalOutcome::Denied => Some(InvalidWaiverReason::ApprovalDenied),
    }
}

fn current_waiver_is_valid(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    finding: &FindingObservation,
) -> bool {
    #[allow(
        clippy::option_if_let_else,
        reason = "explicit Option branches remain directly supported and auditable in Verus"
    )]
    match current_waiver(evidence, finding.finding_id(), requested) {
        Some(waiver) => waiver_failure(contract, requested, evidence, finding, waiver).is_none(),
        None => false,
    }
}

pub(super) fn evaluate_waivers(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    unmet: &mut Vec<UnmetCondition>,
) -> bool {
    let mut complete = true;

    // Validate each supplied current waiver once in canonical waiver order. Validation is not
    // conditional on finding severity: non-blocking findings cannot smuggle unauthorized waiver
    // or approval observations into an otherwise acceptable evidence set.
    let mut waiver_index = 0;
    while waiver_index < evidence.waivers().len()
        invariant 0 <= waiver_index <= evidence.spec_waivers().len(),
        decreases evidence.spec_waivers().len() - waiver_index,
    {
        let waiver = &evidence.waivers()[waiver_index];
        if waiver.revision() == requested {
            match current_finding(evidence, waiver.finding_id(), requested) {
                None => {
                    complete = false;
                    unmet.push(UnmetCondition::InvalidWaiver {
                        finding_id: waiver.finding_id(),
                        reason: InvalidWaiverReason::UnknownFinding,
                    });
                }
                Some(finding) => {
                    if let Some(reason) =
                        waiver_failure(contract, requested, evidence, finding, waiver)
                    {
                        complete = false;
                        unmet.push(UnmetCondition::InvalidWaiver {
                            finding_id: waiver.finding_id(),
                            reason,
                        });
                    }
                }
            }
        }
        waiver_index += 1;
    }

    let threshold = contract.review_policy().blocking_severity();
    let mut review_index = 0;
    while review_index < evidence.reviews().len()
        invariant 0 <= review_index <= evidence.spec_reviews().len(),
        decreases evidence.spec_reviews().len() - review_index,
    {
        if evidence.reviews()[review_index].revision() == requested {
            let findings = evidence.reviews()[review_index].findings();
            let mut finding_index = 0;
            while finding_index < findings.len()
                invariant 0 <= finding_index <= findings.len(),
                decreases findings.len() - finding_index,
            {
                let finding = &findings[finding_index];
                if finding.severity() >= threshold {
                    match finding.disposition() {
                        FindingDisposition::Resolved { .. } => {}
                        FindingDisposition::Open | FindingDisposition::WaiverRequested => {
                            if !current_waiver_is_valid(
                                contract,
                                requested,
                                evidence,
                                finding,
                            ) {
                                complete = false;
                                unmet.push(UnmetCondition::UnwaivedBlocker {
                                    finding_id: finding.finding_id(),
                                    severity: finding.severity(),
                                });
                            }
                        }
                    }
                }
                finding_index += 1;
            }
        }
        review_index += 1;
    }
    complete
}

pub(super) fn evaluate_final_approval(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    unmet: &mut Vec<UnmetCondition>,
) -> bool {
    let required_authority = match contract.approval_policy() {
        HumanApprovalPolicy::NotRequired => return true,
        HumanApprovalPolicy::Required(authority) => authority,
    };
    let mut index = 0;
    while index < evidence.approvals().len()
        invariant 0 <= index <= evidence.spec_approvals().len(),
        decreases evidence.spec_approvals().len() - index,
    {
        let approval = &evidence.approvals()[index];
        if approval.revision() == requested && approval.subject() == ApprovalSubject::Acceptance {
            if approval.authority() != required_authority {
                unmet.push(UnmetCondition::WrongHumanApprovalAuthority);
                return false;
            } else if approval.outcome() == ApprovalOutcome::Denied {
                unmet.push(UnmetCondition::HumanApprovalDenied);
                return false;
            }
            return true;
        }
        index += 1;
    }
    unmet.push(UnmetCondition::MissingHumanApproval);
    false
}

pub(super) fn evaluate_unexpected_approvals(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    unmet: &mut Vec<UnmetCondition>,
) -> bool {
    let mut complete = true;
    let mut index = 0;
    while index < evidence.approvals().len()
        invariant 0 <= index <= evidence.spec_approvals().len(),
        decreases evidence.spec_approvals().len() - index,
    {
        let approval = &evidence.approvals()[index];
        if approval.revision() == requested {
            let expected = match approval.subject() {
                ApprovalSubject::Acceptance => contract.approval_policy().is_required(),
                ApprovalSubject::FindingWaiver(finding_id) => {
                    let mut waiver_index = 0;
                    let mut found = false;
                    while waiver_index < evidence.waivers().len()
                        invariant 0 <= waiver_index <= evidence.spec_waivers().len(),
                        decreases evidence.spec_waivers().len() - waiver_index,
                    {
                        let waiver = &evidence.waivers()[waiver_index];
                        #[allow(
                            clippy::collapsible_if,
                            reason = "separate lookup branch keeps the bounded Verus loop direct"
                        )]
                        if waiver.revision() == requested
                            && waiver.finding_id() == finding_id
                            && waiver.approval_request_id() == approval.request_id()
                        {
                            if let Some(finding) =
                                current_finding(evidence, finding_id, requested)
                            {
                                found = waiver_failure(
                                    contract,
                                    requested,
                                    evidence,
                                    finding,
                                    waiver,
                                )
                                .is_none();
                            }
                        }
                        waiver_index += 1;
                    }
                    found
                }
            };
            if !expected {
                complete = false;
                unmet.push(UnmetCondition::UnexpectedApproval(approval.actor_id()));
            }
        }
        index += 1;
    }
    complete
}

} // verus!
