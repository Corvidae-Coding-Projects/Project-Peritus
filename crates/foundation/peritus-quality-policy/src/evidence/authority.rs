//! Canonical approval subjects and waiver-request consistency.

use crate::{ApprovalObservation, ApprovalSubject, CanonicalEvidenceCollection, EvidenceError, EvidenceErrorKind, WaiverObservation};
#[cfg(verus_only)]
use crate::canonical::collections;
use crate::canonical::order;
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

fn subjects_equal(left: ApprovalSubject, right: ApprovalSubject) -> (equal: bool)
    ensures equal == collections::subjects_match(left, right),
{
    match (left, right) {
        (ApprovalSubject::Acceptance, ApprovalSubject::Acceptance) => true,
        (ApprovalSubject::FindingWaiver(left), ApprovalSubject::FindingWaiver(right)) =>
            matches!(order::compare(left.as_bytes().as_slice(), right.as_bytes().as_slice()), Ordering::Equal),
        _ => false,
    }
}

fn subject_seen_before(values: &[ApprovalObservation], end: usize, target: ApprovalSubject) -> (found: bool)
    requires end <= values.len(),
    ensures found == collections::subject_seen_before(values@, end as int, target),
{
    let mut index = 0;
    while index < end
        invariant
            0 <= index <= end <= values.len(),
            forall |prior: int| 0 <= prior < index ==>
                !collections::subjects_match(#[trigger] values@[prior].spec_subject(), target),
        decreases end - index,
    {
        if subjects_equal(values[index].subject(), target) { return true; }
        index += 1;
    }
    false
}

pub(super) fn validate_approvals(values: &[ApprovalObservation]) -> (result: Result<(), EvidenceError>)
    ensures result.is_ok() == collections::approvals_canonical(values@),
{
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            forall |prior: int| 1 <= prior < index ==>
                order::byte_order(#[trigger] collections::approval_keys(values@)[prior - 1], collections::approval_keys(values@)[prior]) == Ordering::Less,
            forall |prior: int| 0 <= prior < index ==>
                !collections::subject_seen_before(values@, prior, #[trigger] values@[prior].spec_subject()),
        decreases values.len() - index,
    {
        if index > 0 {
            let previous = values[index - 1].request_id().into_bytes();
            let current = values[index].request_id().into_bytes();
            if let Err(error) = order::require_ascending(previous.as_slice(), current.as_slice(), CanonicalEvidenceCollection::Approvals, index) {
                assert(order::byte_order(collections::approval_keys(values@)[index as int - 1], collections::approval_keys(values@)[index as int]) != Ordering::Less);
                assert(!order::ordered(collections::approval_keys(values@)));
                return Err(error);
            }
        }
        if subject_seen_before(values, index, values[index].subject()) {
            assert(collections::subject_seen_before(values@, index as int, values@[index as int].spec_subject()));
            return Err(EvidenceError::new(EvidenceErrorKind::DuplicateApprovalSubject, CanonicalEvidenceCollection::Approvals, index));
        }
        index += 1;
    }
    Ok(())
}

fn waiver_approval_consistent(waiver: &WaiverObservation, approvals: &[ApprovalObservation]) -> (consistent: bool)
    ensures consistent == collections::waiver_approval_consistent(*waiver, approvals@),
{
    let mut index = 0;
    while index < approvals.len()
        invariant
            0 <= index <= approvals.len(),
            forall |prior: int| 0 <= prior < index
                && #[trigger] approvals@[prior].spec_request_id().spec_bytes()@ == waiver.spec_approval_request_id().spec_bytes()@
                ==> collections::subjects_match(approvals@[prior].spec_subject(), ApprovalSubject::FindingWaiver(waiver.spec_finding_id())),
        decreases approvals.len() - index,
    {
        let request = approvals[index].request_id().into_bytes();
        let target = waiver.approval_request_id().into_bytes();
        if matches!(order::compare(request.as_slice(), target.as_slice()), Ordering::Equal)
            && !subjects_equal(approvals[index].subject(), ApprovalSubject::FindingWaiver(waiver.finding_id()))
        {
            assert(approvals@[index as int].spec_request_id().spec_bytes()@ == waiver.spec_approval_request_id().spec_bytes()@);
            return false;
        }
        index += 1;
    }
    true
}

pub(super) fn validate_waivers(values: &[WaiverObservation], approvals: &[ApprovalObservation]) -> (result: Result<(), EvidenceError>)
    ensures result.is_ok() == collections::waivers_canonical(values@, approvals@),
{
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            forall |prior: int| 1 <= prior < index ==>
                order::byte_order(#[trigger] collections::waiver_keys(values@)[prior - 1], collections::waiver_keys(values@)[prior]) == Ordering::Less,
            forall |prior: int| 0 <= prior < index ==>
                collections::waiver_approval_consistent(#[trigger] values@[prior], approvals@),
        decreases values.len() - index,
    {
        if index > 0 {
            let previous = values[index - 1].finding_id().into_bytes();
            let current = values[index].finding_id().into_bytes();
            if let Err(error) = order::require_ascending(previous.as_slice(), current.as_slice(), CanonicalEvidenceCollection::Waivers, index) {
                assert(order::byte_order(collections::waiver_keys(values@)[index as int - 1], collections::waiver_keys(values@)[index as int]) != Ordering::Less);
                assert(!order::ordered(collections::waiver_keys(values@)));
                return Err(error);
            }
        }
        if !waiver_approval_consistent(&values[index], approvals) {
            assert(!collections::waiver_approval_consistent(values@[index as int], approvals@));
            return Err(EvidenceError::new(EvidenceErrorKind::WaiverApprovalSubjectMismatch, CanonicalEvidenceCollection::Waivers, index));
        }
        index += 1;
    }
    Ok(())
}

} // verus!
