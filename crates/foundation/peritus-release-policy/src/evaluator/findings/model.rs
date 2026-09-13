//! Exact finite predicates for supplied findings and waivers.

use crate::{
    FindingDisposition, FindingObservation, ReleaseCandidate, ReleaseEvidence, WaiverObservation,
};
use vstd::prelude::*;

mod state;
pub use state::{
    corresponds, finding_base_step, finding_pair_conflicts, finding_pairs_through, finding_step,
    findings_through, final_state, request_step, requests_through, state_satisfied,
    waiver_base_step, waiver_pair_invalid, waiver_pairs_through, waiver_step, waivers_through,
};

verus! {

pub open spec fn waiver_valid_for(
    waiver: WaiverObservation,
    finding: FindingObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    crate::identity::finding_ids_match(waiver.spec_finding_id(), finding.spec_id())
        && waiver.spec_is_current_for(candidate, evaluated_at)
        && waiver.spec_approved()
        && !crate::identity::principal_ids_match(
            waiver.spec_authority(),
            finding.spec_reporter(),
        )
}

pub open spec fn has_valid_waiver(
    evidence: &ReleaseEvidence,
    finding: FindingObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    exists |index: int| 0 <= index < evidence.spec_waivers().len()
        && #[trigger] waiver_valid_for(
            evidence.spec_waivers()[index],
            finding,
            candidate,
            evaluated_at,
        )
}

pub open spec fn eligible_finding_for(
    finding: FindingObservation,
    waiver: WaiverObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    crate::identity::finding_ids_match(finding.spec_id(), waiver.spec_finding_id())
        && finding.spec_binding().spec_is_current_for(candidate, evaluated_at)
        && finding.spec_disposition() == FindingDisposition::WaiverRequested
        && !finding.spec_release_blocking()
        && !crate::identity::principal_ids_match(
            waiver.spec_authority(),
            finding.spec_reporter(),
        )
}

pub open spec fn waiver_targets_eligible_finding(
    evidence: &ReleaseEvidence,
    waiver: WaiverObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    exists |index: int| 0 <= index < evidence.spec_findings().len()
        && #[trigger] eligible_finding_for(
            evidence.spec_findings()[index],
            waiver,
            candidate,
            evaluated_at,
        )
}

pub open spec fn finding_pair_clear(
    previous: FindingObservation,
    current: FindingObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    !previous.spec_binding().spec_is_current_for(candidate, evaluated_at)
        || !crate::identity::finding_ids_match(previous.spec_id(), current.spec_id())
        || previous.spec_matches(current)
}

pub open spec fn finding_pairs_clear_through(
    values: Seq<FindingObservation>,
    current: FindingObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> bool
    decreases end,
{
    if end == 0 {
        true
    } else {
        finding_pairs_clear_through(
            values,
            current,
            candidate,
            evaluated_at,
            (end - 1) as nat,
        ) && finding_pair_clear(
            values[(end - 1) as int],
            current,
            candidate,
            evaluated_at,
        )
    }
}

pub open spec fn finding_base_clear(
    evidence: &ReleaseEvidence,
    finding: FindingObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    !finding.spec_binding().spec_is_mismatched(candidate)
        && !finding.spec_binding().spec_is_stale_at(candidate, evaluated_at)
        && !(finding.spec_release_blocking()
            && finding.spec_disposition() != FindingDisposition::Resolved)
        && finding.spec_disposition() != FindingDisposition::Open
        && finding.spec_disposition() != FindingDisposition::Ignored
        && finding.spec_disposition() != FindingDisposition::Quarantined
        && (finding.spec_disposition() != FindingDisposition::WaiverRequested
            || (!finding.spec_release_blocking()
                && has_valid_waiver(evidence, finding, candidate, evaluated_at)))
}

pub open spec fn findings_clear_through(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> bool
    decreases end,
{
    if end == 0 {
        true
    } else {
        let finding = evidence.spec_findings()[(end - 1) as int];
        findings_clear_through(evidence, candidate, evaluated_at, (end - 1) as nat)
            && finding_base_clear(evidence, finding, candidate, evaluated_at)
            && finding_pairs_clear_through(
                evidence.spec_findings(),
                finding,
                candidate,
                evaluated_at,
                (end - 1) as nat,
            )
    }
}

pub open spec fn waiver_pair_clear(
    previous: WaiverObservation,
    current: WaiverObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    !crate::identity::finding_ids_match(
        previous.spec_finding_id(),
        current.spec_finding_id(),
    ) || !previous.spec_is_current_for(candidate, evaluated_at)
        || !current.spec_is_current_for(candidate, evaluated_at)
        || previous.spec_matches(current)
}

pub open spec fn waiver_pairs_clear_through(
    values: Seq<WaiverObservation>,
    current: WaiverObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> bool
    decreases end,
{
    if end == 0 {
        true
    } else {
        waiver_pairs_clear_through(
            values,
            current,
            candidate,
            evaluated_at,
            (end - 1) as nat,
        ) && waiver_pair_clear(
            values[(end - 1) as int],
            current,
            candidate,
            evaluated_at,
        )
    }
}

pub open spec fn waiver_base_clear(
    evidence: &ReleaseEvidence,
    waiver: WaiverObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    !waiver.spec_binding().spec_is_mismatched(candidate)
        && !waiver.spec_binding().spec_is_stale_at(candidate, evaluated_at)
        && waiver.spec_approved()
        && waiver_targets_eligible_finding(evidence, waiver, candidate, evaluated_at)
}

pub open spec fn waivers_clear_through(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> bool
    decreases end,
{
    if end == 0 {
        true
    } else {
        let waiver = evidence.spec_waivers()[(end - 1) as int];
        waivers_clear_through(evidence, candidate, evaluated_at, (end - 1) as nat)
            && waiver_base_clear(evidence, waiver, candidate, evaluated_at)
            && waiver_pairs_clear_through(
                evidence.spec_waivers(),
                waiver,
                candidate,
                evaluated_at,
                (end - 1) as nat,
            )
    }
}

pub open spec fn request_clear(
    evidence: &ReleaseEvidence,
    finding: FindingObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    !finding.spec_binding().spec_is_current_for(candidate, evaluated_at)
        || finding.spec_disposition() != FindingDisposition::WaiverRequested
        || finding.spec_release_blocking()
        || has_valid_waiver(evidence, finding, candidate, evaluated_at)
}

pub open spec fn requests_clear_through(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> bool
    decreases end,
{
    if end == 0 {
        true
    } else {
        requests_clear_through(evidence, candidate, evaluated_at, (end - 1) as nat)
            && request_clear(
                evidence,
                evidence.spec_findings()[(end - 1) as int],
                candidate,
                evaluated_at,
            )
    }
}

pub open spec fn findings_satisfied(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    findings_clear_through(
        evidence,
        candidate,
        evaluated_at,
        evidence.spec_findings().len() as nat,
    ) && waivers_clear_through(
        evidence,
        candidate,
        evaluated_at,
        evidence.spec_waivers().len() as nat,
    ) && requests_clear_through(
        evidence,
        candidate,
        evaluated_at,
        evidence.spec_findings().len() as nat,
    )
}

} // verus!
