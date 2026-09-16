//! Exact current-observation lookup over constructor-proved unique identities.

#[cfg(verus_only)]
use crate::model::authority::*;
use crate::{AcceptanceEvidence, ApprovalSubject};
use peritus_types::{ApprovalRequestId, FindingId, RevisionTuple};
use vstd::prelude::*;

verus! {

pub(super) fn finding_equal(left: FindingId, right: FindingId) -> (equal: bool)
    ensures equal == finding_ids_match(left, right),
{
    matches!(crate::canonical::order::compare(left.as_bytes().as_slice(), right.as_bytes().as_slice()), core::cmp::Ordering::Equal)
}

pub(super) fn request_equal(left: ApprovalRequestId, right: ApprovalRequestId) -> (equal: bool)
    ensures equal == request_ids_match(left, right),
{
    matches!(crate::canonical::order::compare(left.as_bytes().as_slice(), right.as_bytes().as_slice()), core::cmp::Ordering::Equal)
}

pub(super) fn subject_equal(left: ApprovalSubject, right: ApprovalSubject) -> (equal: bool)
    ensures equal == crate::canonical::collections::subjects_match(left, right),
{
    match (left, right) {
        (ApprovalSubject::Acceptance, ApprovalSubject::Acceptance) => true,
        (ApprovalSubject::FindingWaiver(left), ApprovalSubject::FindingWaiver(right)) => finding_equal(left, right),
        _ => false,
    }
}

pub(super) fn current_waiver(evidence: &AcceptanceEvidence, finding: FindingId, requested: RevisionTuple) -> (result: Option<usize>)
    ensures match result {
        Some(index) => current_waiver_at(evidence.spec_waivers(), finding, requested, index as int)
            && forall |other: int| current_waiver_at(evidence.spec_waivers(), finding, requested, other) ==> other == index,
        None => forall |index: int| !current_waiver_at(evidence.spec_waivers(), finding, requested, index),
    },
{
    let values = evidence.waivers();
    proof { evidence.canonical_views(); }
    let mut index = 0;
    while index < values.len()
        invariant
            index <= values.len(), values@ == evidence.spec_waivers(),
            crate::canonical::order::unique(crate::canonical::collections::waiver_keys(values@)),
            forall |prior: int| 0 <= prior < index ==> !current_waiver_at(values@, finding, requested, prior),
        decreases values.len() - index,
    {
        if finding_equal(values[index].finding_id(), finding)
            && crate::revision::revision_matches(values[index].revision(), requested) {
            assert forall |other: int| current_waiver_at(values@, finding, requested, other) implies other == index by {
                let keys = crate::canonical::collections::waiver_keys(values@);
                assert(keys[other] == keys[index as int]);
            }
            return Some(index);
        }
        index += 1;
    }
    None
}

pub(super) fn current_approval(evidence: &AcceptanceEvidence, request: ApprovalRequestId, requested: RevisionTuple) -> (result: Option<usize>)
    ensures match result {
        Some(index) => current_approval_at(evidence.spec_approvals(), request, requested, index as int)
            && forall |other: int| current_approval_at(evidence.spec_approvals(), request, requested, other) ==> other == index,
        None => forall |index: int| !current_approval_at(evidence.spec_approvals(), request, requested, index),
    },
{
    let values = evidence.approvals();
    proof { evidence.canonical_views(); }
    let mut index = 0;
    while index < values.len()
        invariant
            index <= values.len(), values@ == evidence.spec_approvals(),
            crate::canonical::order::unique(crate::canonical::collections::approval_keys(values@)),
            forall |prior: int| 0 <= prior < index ==> !current_approval_at(values@, request, requested, prior),
        decreases values.len() - index,
    {
        if request_equal(values[index].request_id(), request)
            && crate::revision::revision_matches(values[index].revision(), requested) {
            assert forall |other: int| current_approval_at(values@, request, requested, other) implies other == index by {
                let keys = crate::canonical::collections::approval_keys(values@);
                assert(keys[other] == keys[index as int]);
            }
            return Some(index);
        }
        index += 1;
    }
    None
}

proof fn unique_finding(values: Seq<crate::ReviewObservation>, review: int, index: int, other_review: int, other_index: int)
    requires
        crate::canonical::collections::reviews_canonical(values),
        0 <= review < values.len(), 0 <= other_review < values.len(),
        0 <= index < values[review].spec_findings().len(),
        0 <= other_index < values[other_review].spec_findings().len(),
        finding_ids_match(values[review].spec_findings()[index].spec_finding_id(), values[other_review].spec_findings()[other_index].spec_finding_id()),
    ensures review == other_review && index == other_index,
{
    if review < other_review {
        assert(crate::canonical::collections::review_findings_unique_at(values, other_review));
        assert(crate::canonical::collections::finding_seen_before(values, other_review, values[other_review].spec_findings()[other_index].spec_finding_id().spec_bytes()@));
    } else if other_review < review {
        assert(crate::canonical::collections::review_findings_unique_at(values, review));
        assert(crate::canonical::collections::finding_seen_before(values, review, values[review].spec_findings()[index].spec_finding_id().spec_bytes()@));
    } else {
        values[review].canonical_views();
        crate::canonical::reviews::canonical_implies_unique(values[review].spec_categories(), values[review].spec_findings(), values[review].spec_revision());
        let keys = crate::canonical::reviews::finding_keys(values[review].spec_findings());
        assert(keys[index] == keys[other_index]);
    }
}

pub(super) fn current_finding(evidence: &AcceptanceEvidence, finding: FindingId, requested: RevisionTuple) -> (result: Option<(usize, usize)>)
    ensures match result {
        Some((review, index)) => current_finding_at(evidence.spec_reviews(), finding, requested, review as int, index as int)
            && forall |other_review: int, other_index: int| current_finding_at(evidence.spec_reviews(), finding, requested, other_review, other_index) ==> other_review == review && other_index == index,
        None => forall |review: int, index: int| !current_finding_at(evidence.spec_reviews(), finding, requested, review, index),
    },
{
    let reviews = evidence.reviews();
    proof { evidence.canonical_views(); }
    let mut review_index = 0;
    while review_index < reviews.len()
        invariant
            review_index <= reviews.len(), reviews@ == evidence.spec_reviews(),
            crate::canonical::collections::reviews_canonical(reviews@),
            forall |review: int, index: int| 0 <= review < review_index ==> !current_finding_at(reviews@, finding, requested, review, index),
        decreases reviews.len() - review_index,
    {
        if crate::revision::revision_matches(reviews[review_index].revision(), requested) {
            let findings = reviews[review_index].findings();
            let mut index = 0;
            while index < findings.len()
                invariant
                    index <= findings.len(), review_index < reviews.len(), reviews@ == evidence.spec_reviews(),
                    findings@ == reviews@[review_index as int].spec_findings(),
                    crate::canonical::collections::reviews_canonical(reviews@),
                    crate::model::revision_fresh(reviews@[review_index as int].spec_revision(), requested),
                    forall |review: int, prior: int| 0 <= review < review_index ==> !current_finding_at(reviews@, finding, requested, review, prior),
                    forall |prior: int| 0 <= prior < index ==> !current_finding_at(reviews@, finding, requested, review_index as int, prior),
                decreases findings.len() - index,
            {
                if finding_equal(findings[index].finding_id(), finding) {
                    assert forall |other_review: int, other_index: int| current_finding_at(reviews@, finding, requested, other_review, other_index) implies other_review == review_index && other_index == index by {
                        unique_finding(reviews@, review_index as int, index as int, other_review, other_index);
                    }
                    return Some((review_index, index));
                }
                index += 1;
            }
        }
        review_index += 1;
    }
    None
}

} // verus!
