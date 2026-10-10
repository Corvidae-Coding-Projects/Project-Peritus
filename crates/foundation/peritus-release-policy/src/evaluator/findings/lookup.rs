//! Exact executable lookup predicates for finding waivers.

use crate::{
    FindingDisposition, FindingObservation, ReleaseCandidate, ReleaseEvidence, WaiverObservation,
};

#[allow(
    clippy::large_types_passed_by_value,
    reason = "finding and candidate are immutable Copy policy values"
)]
pub(super) fn has_valid_waiver(
    finding: FindingObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> bool {
    let mut index = 0;
    while index < evidence.waivers().len() {
        let waiver = evidence.waivers()[index];
        if crate::identity::finding_ids_equal(waiver.finding_id(), finding.id())
            && waiver.is_current_for(candidate, evaluated_at)
            && waiver.approved()
            && !crate::identity::principal_ids_equal(waiver.authority(), finding.reporter())
        {
            return true;
        }
        index += 1;
    }
    false
}
#[allow(
    clippy::large_types_passed_by_value,
    reason = "waiver and candidate are immutable Copy policy values"
)]
pub(super) fn waiver_targets_eligible_finding(
    waiver: WaiverObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> bool {
    let mut index = 0;
    while index < evidence.findings().len() {
        let finding = evidence.findings()[index];
        if crate::identity::finding_ids_equal(finding.id(), waiver.finding_id())
            && finding.binding().is_current_for(candidate, evaluated_at)
            && crate::review::finding_dispositions_equal(
                finding.disposition(),
                FindingDisposition::WaiverRequested,
            )
            && !finding.release_blocking()
            && !crate::identity::principal_ids_equal(waiver.authority(), finding.reporter())
        {
            return true;
        }
        index += 1;
    }
    false
}
