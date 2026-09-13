//! Exact executable lookup predicates for finding waivers.

#[cfg(verus_only)]
use super::model;
use crate::{FindingDisposition, FindingObservation, ReleaseCandidate, ReleaseEvidence, WaiverObservation};
use vstd::prelude::*;

verus! {

#[allow(clippy::large_types_passed_by_value, reason = "finding and candidate are immutable Copy policy values")]
pub(super) fn has_valid_waiver(
    finding: FindingObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> (found: bool)
    ensures found == model::has_valid_waiver(evidence, finding, candidate, evaluated_at),
{
    let mut index = 0;
    while index < evidence.waivers().len()
        invariant
            0 <= index <= evidence.spec_waivers().len(),
            forall |prior: int| 0 <= prior < index ==> !#[trigger] model::waiver_valid_for(
                evidence.spec_waivers()[prior],
                finding,
                candidate,
                evaluated_at,
            ),
        decreases evidence.spec_waivers().len() - index,
    {
        let waiver = evidence.waivers()[index];
        if crate::identity::finding_ids_equal(waiver.finding_id(), finding.id())
            && waiver.is_current_for(candidate, evaluated_at)
            && waiver.approved()
            && !crate::identity::principal_ids_equal(waiver.authority(), finding.reporter())
        {
            proof {
                reveal(model::waiver_valid_for);
                reveal(model::has_valid_waiver);
                reveal(WaiverObservation::spec_is_current_for);
                assert(waiver == evidence.spec_waivers()[index as int]);
                assert(model::waiver_valid_for(waiver, finding, candidate, evaluated_at));
                assert(exists |witness: int| witness == index as int
                    && 0 <= witness < evidence.spec_waivers().len()
                    && #[trigger] model::waiver_valid_for(
                        evidence.spec_waivers()[witness],
                        finding,
                        candidate,
                        evaluated_at,
                    ));
            }
            return true;
        }
        proof {
            reveal(model::waiver_valid_for);
        }
        index += 1;
    }
    proof {
        reveal(model::has_valid_waiver);
    }
    false
}

#[allow(clippy::large_types_passed_by_value, reason = "waiver and candidate are immutable Copy policy values")]
pub(super) fn waiver_targets_eligible_finding(
    waiver: WaiverObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> (found: bool)
    ensures found
        == model::waiver_targets_eligible_finding(evidence, waiver, candidate, evaluated_at),
{
    let mut index = 0;
    while index < evidence.findings().len()
        invariant
            0 <= index <= evidence.spec_findings().len(),
            forall |prior: int| 0 <= prior < index ==> !#[trigger] model::eligible_finding_for(
                evidence.spec_findings()[prior],
                waiver,
                candidate,
                evaluated_at,
            ),
        decreases evidence.spec_findings().len() - index,
    {
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
            proof {
                reveal(model::eligible_finding_for);
                reveal(model::waiver_targets_eligible_finding);
                assert(finding == evidence.spec_findings()[index as int]);
                assert(model::eligible_finding_for(
                    finding,
                    waiver,
                    candidate,
                    evaluated_at,
                ));
                assert(exists |witness: int| witness == index as int
                    && 0 <= witness < evidence.spec_findings().len()
                    && #[trigger] model::eligible_finding_for(
                        evidence.spec_findings()[witness],
                        waiver,
                        candidate,
                        evaluated_at,
                    ));
            }
            return true;
        }
        proof {
            reveal(model::eligible_finding_for);
        }
        index += 1;
    }
    proof {
        reveal(model::waiver_targets_eligible_finding);
    }
    false
}


} // verus!
