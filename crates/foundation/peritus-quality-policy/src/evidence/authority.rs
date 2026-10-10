//! Canonical approval subjects and waiver-request consistency.

use crate::{
    ApprovalObservation, ApprovalSubject, CanonicalEvidenceCollection, EvidenceError,
    EvidenceErrorKind, WaiverObservation,
};

use crate::canonical::order;
use core::cmp::Ordering;

fn subjects_equal(left: ApprovalSubject, right: ApprovalSubject) -> bool {
    match (left, right) {
        (ApprovalSubject::Acceptance, ApprovalSubject::Acceptance) => true,
        (ApprovalSubject::FindingWaiver(left), ApprovalSubject::FindingWaiver(right)) => matches!(
            order::compare(left.as_bytes().as_slice(), right.as_bytes().as_slice()),
            Ordering::Equal
        ),
        _ => false,
    }
}
fn subject_seen_before(
    values: &[ApprovalObservation],
    end: usize,
    target: ApprovalSubject,
) -> bool {
    let mut index = 0;
    while index < end {
        if subjects_equal(values[index].subject(), target) {
            return true;
        }
        index += 1;
    }
    false
}
pub(super) fn validate_approvals(values: &[ApprovalObservation]) -> Result<(), EvidenceError> {
    let mut index = 0;
    while index < values.len() {
        if index > 0 {
            let previous = values[index - 1].request_id().into_bytes();
            let current = values[index].request_id().into_bytes();
            if let Err(error) = order::require_ascending(
                previous.as_slice(),
                current.as_slice(),
                CanonicalEvidenceCollection::Approvals,
                index,
            ) {
                return Err(error);
            }
        }
        if subject_seen_before(values, index, values[index].subject()) {
            return Err(EvidenceError::new(
                EvidenceErrorKind::DuplicateApprovalSubject,
                CanonicalEvidenceCollection::Approvals,
                index,
            ));
        }
        index += 1;
    }
    Ok(())
}
fn waiver_approval_consistent(
    waiver: &WaiverObservation,
    approvals: &[ApprovalObservation],
) -> bool {
    let mut index = 0;
    while index < approvals.len() {
        let request = approvals[index].request_id().into_bytes();
        let target = waiver.approval_request_id().into_bytes();
        if matches!(order::compare(request.as_slice(), target.as_slice()), Ordering::Equal)
            && !subjects_equal(
                approvals[index].subject(),
                ApprovalSubject::FindingWaiver(waiver.finding_id()),
            )
        {
            return false;
        }
        index += 1;
    }
    true
}
pub(super) fn validate_waivers(
    values: &[WaiverObservation],
    approvals: &[ApprovalObservation],
) -> Result<(), EvidenceError> {
    let mut index = 0;
    while index < values.len() {
        if index > 0 {
            let previous = values[index - 1].finding_id().into_bytes();
            let current = values[index].finding_id().into_bytes();
            if let Err(error) = order::require_ascending(
                previous.as_slice(),
                current.as_slice(),
                CanonicalEvidenceCollection::Waivers,
                index,
            ) {
                return Err(error);
            }
        }
        if !waiver_approval_consistent(&values[index], approvals) {
            return Err(EvidenceError::new(
                EvidenceErrorKind::WaiverApprovalSubjectMismatch,
                CanonicalEvidenceCollection::Waivers,
                index,
            ));
        }
        index += 1;
    }
    Ok(())
}
