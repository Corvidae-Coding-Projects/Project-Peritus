//! Exact evidence searches used by finding-waiver transitions.

use peritus_quality_policy::{AcceptanceEvidence, UnmetCondition};
use peritus_types::{FindingId, ReviewCycleId, RevisionTuple};

pub(super) fn current_supplied_waiver(
    evidence: &AcceptanceEvidence,
    finding_id: FindingId,
    revision: RevisionTuple,
) -> Option<usize> {
    let waivers = evidence.waivers();
    let mut index = 0;
    while index < waivers.len() {
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
) -> Option<(usize, usize)> {
    let reviews = evidence.reviews();
    let mut review_index = 0;
    while review_index < reviews.len() {
        let review = &reviews[review_index];
        if crate::identity::review_cycle_id_equal(review.cycle_id(), review_id)
            && crate::identity::revision_equal(review.revision(), revision)
        {
            let findings = review.findings();
            let mut finding_index = 0;
            while finding_index < findings.len() {
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
) -> bool {
    let mut index = 0;
    while index < conditions.len() {
        if let UnmetCondition::InvalidWaiver { finding_id: target, .. } = conditions[index] {
            if crate::identity::finding_id_equal(target, finding_id) {
                return true;
            }
        }
        index += 1;
    }
    false
}
