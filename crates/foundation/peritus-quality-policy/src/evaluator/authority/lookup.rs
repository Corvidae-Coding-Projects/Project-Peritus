//! Exact current-observation lookup over constructor-proved unique identities.

use crate::{AcceptanceEvidence, ApprovalSubject};
use peritus_types::{ApprovalRequestId, FindingId, RevisionTuple};

pub(super) fn finding_equal(left: FindingId, right: FindingId) -> bool {
    matches!(
        crate::canonical::order::compare(left.as_bytes().as_slice(), right.as_bytes().as_slice()),
        core::cmp::Ordering::Equal
    )
}
pub(super) fn request_equal(left: ApprovalRequestId, right: ApprovalRequestId) -> bool {
    matches!(
        crate::canonical::order::compare(left.as_bytes().as_slice(), right.as_bytes().as_slice()),
        core::cmp::Ordering::Equal
    )
}
pub(super) fn subject_equal(left: ApprovalSubject, right: ApprovalSubject) -> bool {
    match (left, right) {
        (ApprovalSubject::Acceptance, ApprovalSubject::Acceptance) => true,
        (ApprovalSubject::FindingWaiver(left), ApprovalSubject::FindingWaiver(right)) => {
            finding_equal(left, right)
        }
        _ => false,
    }
}
pub(super) fn current_waiver(
    evidence: &AcceptanceEvidence,
    finding: FindingId,
    requested: RevisionTuple,
) -> Option<usize> {
    let values = evidence.waivers();
    let mut index = 0;
    while index < values.len() {
        if finding_equal(values[index].finding_id(), finding)
            && crate::revision::revision_matches(values[index].revision(), requested)
        {
            return Some(index);
        }
        index += 1;
    }
    None
}
pub(super) fn current_approval(
    evidence: &AcceptanceEvidence,
    request: ApprovalRequestId,
    requested: RevisionTuple,
) -> Option<usize> {
    let values = evidence.approvals();
    let mut index = 0;
    while index < values.len() {
        if request_equal(values[index].request_id(), request)
            && crate::revision::revision_matches(values[index].revision(), requested)
        {
            return Some(index);
        }
        index += 1;
    }
    None
}
pub(super) fn current_finding(
    evidence: &AcceptanceEvidence,
    finding: FindingId,
    requested: RevisionTuple,
) -> Option<(usize, usize)> {
    let reviews = evidence.reviews();
    let mut review_index = 0;
    while review_index < reviews.len() {
        if crate::revision::revision_matches(reviews[review_index].revision(), requested) {
            let findings = reviews[review_index].findings();
            let mut index = 0;
            while index < findings.len() {
                if finding_equal(findings[index].finding_id(), finding) {
                    return Some((review_index, index));
                }
                index += 1;
            }
        }
        review_index += 1;
    }
    None
}
