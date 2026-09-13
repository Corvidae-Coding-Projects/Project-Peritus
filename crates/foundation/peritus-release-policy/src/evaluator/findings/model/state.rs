//! Exact saturated finding and waiver reducer state.

use crate::{FindingDisposition, FindingObservation, ReleaseCandidate, ReleaseEvidence, WaiverObservation};
use vstd::prelude::*;

verus! {

pub struct FindingState {
    pub stale_count: u16,
    pub mismatched_count: u16,
    pub open_count: u16,
    pub release_blocking_count: u16,
    pub ignored_count: u16,
    pub quarantined_count: u16,
    pub invalid_waiver_count: u16,
    pub conflicting_finding: bool,
}

pub open spec fn initial() -> FindingState {
    FindingState {
        stale_count: 0,
        mismatched_count: 0,
        open_count: 0,
        release_blocking_count: 0,
        ignored_count: 0,
        quarantined_count: 0,
        invalid_waiver_count: 0,
        conflicting_finding: false,
    }
}

pub open spec fn finding_base_step(
    state: FindingState,
    evidence: &ReleaseEvidence,
    finding: FindingObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> FindingState {
    if finding.spec_binding().spec_is_mismatched(candidate) {
        FindingState {
            mismatched_count: super::super::super::saturated_increment(state.mismatched_count),
            ..state
        }
    } else if finding.spec_binding().spec_is_stale_at(candidate, evaluated_at) {
        FindingState {
            stale_count: super::super::super::saturated_increment(state.stale_count),
            ..state
        }
    } else {
        let unresolved_blocker = finding.spec_release_blocking()
            && finding.spec_disposition() != FindingDisposition::Resolved;
        let open = finding.spec_disposition() == FindingDisposition::Open
            || (finding.spec_disposition() == FindingDisposition::WaiverRequested
                && (finding.spec_release_blocking()
                    || !super::has_valid_waiver(evidence, finding, candidate, evaluated_at)));
        FindingState {
            open_count: if open {
                super::super::super::saturated_increment(state.open_count)
            } else { state.open_count },
            release_blocking_count: if unresolved_blocker {
                super::super::super::saturated_increment(state.release_blocking_count)
            } else { state.release_blocking_count },
            ignored_count: if finding.spec_disposition() == FindingDisposition::Ignored {
                super::super::super::saturated_increment(state.ignored_count)
            } else { state.ignored_count },
            quarantined_count: if finding.spec_disposition() == FindingDisposition::Quarantined {
                super::super::super::saturated_increment(state.quarantined_count)
            } else { state.quarantined_count },
            ..state
        }
    }
}

pub open spec fn finding_pair_conflicts(
    previous: FindingObservation,
    current: FindingObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    previous.spec_binding().spec_is_current_for(candidate, evaluated_at)
        && crate::identity::finding_ids_match(previous.spec_id(), current.spec_id())
        && !previous.spec_matches(current)
}

pub open spec fn finding_pairs_through(
    state: FindingState,
    values: Seq<FindingObservation>,
    current: FindingObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> FindingState
    decreases end,
{
    if end == 0 {
        state
    } else {
        let prior = finding_pairs_through(
            state, values, current, candidate, evaluated_at, (end - 1) as nat);
        FindingState {
            conflicting_finding: prior.conflicting_finding || finding_pair_conflicts(
                values[(end - 1) as int], current, candidate, evaluated_at),
            ..prior
        }
    }
}

pub open spec fn finding_step(
    state: FindingState,
    evidence: &ReleaseEvidence,
    index: nat,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> FindingState {
    let finding = evidence.spec_findings()[index as int];
    let base = finding_base_step(state, evidence, finding, candidate, evaluated_at);
    if finding.spec_binding().spec_is_mismatched(candidate)
        || finding.spec_binding().spec_is_stale_at(candidate, evaluated_at)
    {
        base
    } else {
        finding_pairs_through(
            base, evidence.spec_findings(), finding, candidate, evaluated_at, index)
    }
}

pub open spec fn findings_through(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> FindingState
    decreases end,
{
    if end == 0 {
        initial()
    } else {
        finding_step(
            findings_through(evidence, candidate, evaluated_at, (end - 1) as nat),
            evidence,
            (end - 1) as nat,
            candidate,
            evaluated_at,
        )
    }
}

pub open spec fn waiver_base_step(
    state: FindingState,
    evidence: &ReleaseEvidence,
    waiver: WaiverObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> FindingState {
    if waiver.spec_binding().spec_is_mismatched(candidate) {
        FindingState {
            mismatched_count: super::super::super::saturated_increment(state.mismatched_count),
            invalid_waiver_count: super::super::super::saturated_increment(
                state.invalid_waiver_count),
            ..state
        }
    } else if waiver.spec_binding().spec_is_stale_at(candidate, evaluated_at) {
        FindingState {
            stale_count: super::super::super::saturated_increment(state.stale_count),
            invalid_waiver_count: super::super::super::saturated_increment(
                state.invalid_waiver_count),
            ..state
        }
    } else if !waiver.spec_approved()
        || !super::waiver_targets_eligible_finding(evidence, waiver, candidate, evaluated_at)
    {
        FindingState {
            invalid_waiver_count: super::super::super::saturated_increment(
                state.invalid_waiver_count),
            ..state
        }
    } else {
        state
    }
}

pub open spec fn waiver_pair_invalid(
    previous: WaiverObservation,
    current: WaiverObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    crate::identity::finding_ids_match(previous.spec_finding_id(), current.spec_finding_id())
        && previous.spec_is_current_for(candidate, evaluated_at)
        && current.spec_is_current_for(candidate, evaluated_at)
        && !previous.spec_matches(current)
}

pub open spec fn waiver_pairs_through(
    state: FindingState,
    values: Seq<WaiverObservation>,
    current: WaiverObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> FindingState
    decreases end,
{
    if end == 0 {
        state
    } else {
        let prior = waiver_pairs_through(
            state, values, current, candidate, evaluated_at, (end - 1) as nat);
        if waiver_pair_invalid(values[(end - 1) as int], current, candidate, evaluated_at) {
            FindingState {
                invalid_waiver_count: super::super::super::saturated_increment(
                    prior.invalid_waiver_count),
                ..prior
            }
        } else {
            prior
        }
    }
}

pub open spec fn waiver_step(
    state: FindingState,
    evidence: &ReleaseEvidence,
    index: nat,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> FindingState {
    let waiver = evidence.spec_waivers()[index as int];
    waiver_pairs_through(
        waiver_base_step(state, evidence, waiver, candidate, evaluated_at),
        evidence.spec_waivers(),
        waiver,
        candidate,
        evaluated_at,
        index,
    )
}

pub open spec fn waivers_through(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> FindingState
    decreases end,
{
    if end == 0 {
        findings_through(evidence, candidate, evaluated_at, evidence.spec_findings().len() as nat)
    } else {
        waiver_step(
            waivers_through(evidence, candidate, evaluated_at, (end - 1) as nat),
            evidence,
            (end - 1) as nat,
            candidate,
            evaluated_at,
        )
    }
}

pub open spec fn request_step(
    state: FindingState,
    evidence: &ReleaseEvidence,
    finding: FindingObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> FindingState {
    if finding.spec_binding().spec_is_current_for(candidate, evaluated_at)
        && finding.spec_disposition() == FindingDisposition::WaiverRequested
        && !finding.spec_release_blocking()
        && !super::has_valid_waiver(evidence, finding, candidate, evaluated_at)
    {
        FindingState {
            invalid_waiver_count: super::super::super::saturated_increment(
                state.invalid_waiver_count),
            ..state
        }
    } else {
        state
    }
}

pub open spec fn requests_through(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> FindingState
    decreases end,
{
    if end == 0 {
        waivers_through(evidence, candidate, evaluated_at, evidence.spec_waivers().len() as nat)
    } else {
        request_step(
            requests_through(evidence, candidate, evaluated_at, (end - 1) as nat),
            evidence,
            evidence.spec_findings()[(end - 1) as int],
            candidate,
            evaluated_at,
        )
    }
}

pub open spec fn final_state(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> FindingState {
    requests_through(evidence, candidate, evaluated_at, evidence.spec_findings().len() as nat)
}

pub open spec fn state_satisfied(state: FindingState) -> bool {
    state.stale_count == 0
        && state.mismatched_count == 0
        && state.open_count == 0
        && state.release_blocking_count == 0
        && state.ignored_count == 0
        && state.quarantined_count == 0
        && state.invalid_waiver_count == 0
        && !state.conflicting_finding
}

pub open spec fn corresponds(
    state: FindingState,
    stale_count: u16,
    mismatched_count: u16,
    open_count: u16,
    release_blocking_count: u16,
    ignored_count: u16,
    quarantined_count: u16,
    invalid_waiver_count: u16,
    conflicting_finding: bool,
) -> bool {
    state.stale_count == stale_count
        && state.mismatched_count == mismatched_count
        && state.open_count == open_count
        && state.release_blocking_count == release_blocking_count
        && state.ignored_count == ignored_count
        && state.quarantined_count == quarantined_count
        && state.invalid_waiver_count == invalid_waiver_count
        && state.conflicting_finding == conflicting_finding
}

} // verus!
