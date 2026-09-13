//! Exact evidence searches used by finding-waiver transitions.

use peritus_quality_policy::{AcceptanceEvidence, UnmetCondition};
use peritus_types::{FindingId, ReviewCycleId, RevisionTuple};
use vstd::prelude::*;

verus! {

pub(super) fn current_supplied_waiver(
    evidence: &AcceptanceEvidence,
    finding_id: FindingId,
    revision: RevisionTuple,
) -> (result: Option<usize>)
    ensures match result {
        Some(index) => peritus_quality_policy::current_waiver_at(
            evidence.spec_waivers(), finding_id, revision, index as int),
        None => forall |index: int|
            !peritus_quality_policy::current_waiver_at(
                evidence.spec_waivers(), finding_id, revision, index),
    },
{
    let waivers = evidence.waivers();
    let mut index = 0;
    while index < waivers.len()
        invariant
            index <= waivers.len(),
            waivers@ == evidence.spec_waivers(),
            forall |prior: int| 0 <= prior < index ==>
                !peritus_quality_policy::current_waiver_at(
                    waivers@, finding_id, revision, prior),
        decreases waivers.len() - index,
    {
        if crate::identity::finding_id_equal(waivers[index].finding_id(), finding_id)
            && crate::identity::revision_equal(waivers[index].revision(), revision)
        {
            return Some(index);
        }
        index += 1;
    }
    None
}

pub(super) fn current_requested_finding(
    evidence: &AcceptanceEvidence,
    review_id: ReviewCycleId,
    finding_id: FindingId,
    revision: RevisionTuple,
) -> (result: Option<(usize, usize)>)
    ensures match result {
        Some((review, finding)) =>
            peritus_quality_policy::current_finding_at(
                evidence.spec_reviews(), finding_id, revision, review as int, finding as int)
            && evidence.spec_reviews()[review as int].spec_cycle_id().spec_bytes()@
                == review_id.spec_bytes()@,
        None => true,
    },
{
    let reviews = evidence.reviews();
    let mut review_index = 0;
    while review_index < reviews.len()
        invariant review_index <= reviews.len(), reviews@ == evidence.spec_reviews(),
        decreases reviews.len() - review_index,
    {
        let review = &reviews[review_index];
        if crate::identity::review_cycle_id_equal(review.cycle_id(), review_id)
            && crate::identity::revision_equal(review.revision(), revision)
        {
            let findings = review.findings();
            let mut finding_index = 0;
            while finding_index < findings.len()
                invariant
                    finding_index <= findings.len(),
                    review_index < reviews.len(),
                    reviews@ == evidence.spec_reviews(),
                    findings@ == reviews@[review_index as int].spec_findings(),
                    reviews@[review_index as int].spec_cycle_id().spec_bytes()@
                        == review_id.spec_bytes()@,
                    peritus_quality_policy::revision_fresh(
                        reviews@[review_index as int].spec_revision(), revision),
                decreases findings.len() - finding_index,
            {
                if crate::identity::finding_id_equal(
                    findings[finding_index].finding_id(),
                    finding_id,
                ) {
                    return Some((review_index, finding_index));
                }
                finding_index += 1;
            }
        }
        review_index += 1;
    }
    None
}

#[allow(
    clippy::collapsible_if,
    reason = "the explicit nested branch stays within the supported Verus execution subset"
)]
pub(super) fn invalid_waiver_is_reported(
    conditions: &[UnmetCondition],
    finding_id: FindingId,
) -> (reported: bool)
    ensures reported == peritus_quality_policy::invalid_waiver_reported(conditions@, finding_id),
{
    let mut index = 0;
    while index < conditions.len()
        invariant
            index <= conditions.len(),
            forall |prior: int| 0 <= prior < index ==> match #[trigger] conditions@[prior] {
                UnmetCondition::InvalidWaiver { finding_id: target, .. } =>
                    !peritus_quality_policy::finding_ids_match(target, finding_id),
                _ => true,
            },
        decreases conditions.len() - index,
    {
        if let UnmetCondition::InvalidWaiver { finding_id: target, .. } = conditions[index] {
            if crate::identity::finding_id_equal(target, finding_id) {
                return true;
            }
        }
        index += 1;
    }
    false
}

} // verus!
