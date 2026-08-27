//! Finding and waiver policy reduction.

use crate::{
    FindingAssessment, FindingDisposition, FindingObservation, ReleaseCandidate, ReleaseEvidence,
    WaiverObservation,
};
use vstd::prelude::*;

verus! {

#[allow(
    clippy::large_types_passed_by_value,
    clippy::too_many_lines,
    reason = "one auditable phase keeps finding and waiver interactions explicit"
)]
pub(super) fn assess(
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> FindingAssessment {
    let mut stale_count = 0u16;
    let mut mismatched_count = 0u16;
    let mut open_count = 0u16;
    let mut release_blocking_count = 0u16;
    let mut ignored_count = 0u16;
    let mut quarantined_count = 0u16;
    let mut invalid_waiver_count = 0u16;
    let mut conflicting_finding = false;

    let mut right = 0;
    while right < evidence.findings().len()
        invariant 0 <= right <= evidence.spec_findings().len(),
        decreases evidence.spec_findings().len() - right,
    {
        let finding = evidence.findings()[right];
        if finding.binding().is_mismatched(candidate) {
            super::increment(&mut mismatched_count);
        } else if finding.binding().is_stale_at(candidate, evaluated_at) {
            super::increment(&mut stale_count);
        } else {
            let resolved = matches!(finding.disposition(), FindingDisposition::Resolved);
            if finding.release_blocking() && !resolved {
                super::increment(&mut release_blocking_count);
            }
            match finding.disposition() {
                FindingDisposition::Open => super::increment(&mut open_count),
                FindingDisposition::Resolved => {}
                FindingDisposition::Ignored => super::increment(&mut ignored_count),
                FindingDisposition::Quarantined => super::increment(&mut quarantined_count),
                FindingDisposition::WaiverRequested => {
                    if finding.release_blocking()
                        || !has_valid_waiver(finding, candidate, evaluated_at, evidence)
                    {
                        super::increment(&mut open_count);
                    }
                }
            }
            let mut left = 0;
            while left < right
                invariant
                    0 <= left <= right,
                    right < evidence.spec_findings().len(),
                decreases right - left,
            {
                let previous = evidence.findings()[left];
                if previous.binding().is_current_for(candidate, evaluated_at)
                    && previous.id() == finding.id()
                    && previous != finding
                {
                    conflicting_finding = true;
                }
                left += 1;
            }
        }
        right += 1;
    }

    let mut waiver_index = 0;
    while waiver_index < evidence.waivers().len()
        invariant 0 <= waiver_index <= evidence.spec_waivers().len(),
        decreases evidence.spec_waivers().len() - waiver_index,
    {
        let waiver = evidence.waivers()[waiver_index];
        if waiver.binding().is_mismatched(candidate) {
            super::increment(&mut mismatched_count);
            super::increment(&mut invalid_waiver_count);
        } else if waiver.binding().is_stale_at(candidate, evaluated_at) {
            super::increment(&mut stale_count);
            super::increment(&mut invalid_waiver_count);
        } else if !waiver.approved()
            || !waiver_targets_eligible_finding(waiver, candidate, evaluated_at, evidence)
        {
            super::increment(&mut invalid_waiver_count);
        }
        let mut earlier_waiver = 0;
        while earlier_waiver < waiver_index
            invariant
                0 <= earlier_waiver <= waiver_index,
                waiver_index < evidence.spec_waivers().len(),
            decreases waiver_index - earlier_waiver,
        {
            let previous = evidence.waivers()[earlier_waiver];
            if previous.finding_id() == waiver.finding_id()
                && previous.is_current_for(candidate, evaluated_at)
                && waiver.is_current_for(candidate, evaluated_at)
                && previous != waiver
            {
                super::increment(&mut invalid_waiver_count);
            }
            earlier_waiver += 1;
        }
        waiver_index += 1;
    }

    let mut finding_index = 0;
    while finding_index < evidence.findings().len()
        invariant 0 <= finding_index <= evidence.spec_findings().len(),
        decreases evidence.spec_findings().len() - finding_index,
    {
        let finding = evidence.findings()[finding_index];
        if finding.binding().is_current_for(candidate, evaluated_at)
            && matches!(finding.disposition(), FindingDisposition::WaiverRequested)
            && !finding.release_blocking()
            && !has_valid_waiver(finding, candidate, evaluated_at, evidence)
        {
            super::increment(&mut invalid_waiver_count);
        }
        finding_index += 1;
    }

    let satisfied = stale_count == 0
        && mismatched_count == 0
        && open_count == 0
        && release_blocking_count == 0
        && ignored_count == 0
        && quarantined_count == 0
        && invalid_waiver_count == 0
        && !conflicting_finding;
    FindingAssessment::new(
        satisfied,
        stale_count,
        mismatched_count,
        open_count,
        release_blocking_count,
        ignored_count,
        quarantined_count,
        invalid_waiver_count,
        conflicting_finding,
    )
}

#[allow(clippy::large_types_passed_by_value, reason = "finding and candidate are immutable Copy policy values")]
fn has_valid_waiver(
    finding: FindingObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> bool {
    let mut index = 0;
    while index < evidence.waivers().len()
        invariant 0 <= index <= evidence.spec_waivers().len(),
        decreases evidence.spec_waivers().len() - index,
    {
        let waiver = evidence.waivers()[index];
        if waiver.finding_id() == finding.id()
            && waiver.is_current_for(candidate, evaluated_at)
            && waiver.approved()
            && waiver.authority() != finding.reporter()
        {
            return true;
        }
        index += 1;
    }
    false
}

#[allow(clippy::large_types_passed_by_value, reason = "waiver and candidate are immutable Copy policy values")]
fn waiver_targets_eligible_finding(
    waiver: WaiverObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> bool {
    let mut index = 0;
    while index < evidence.findings().len()
        invariant 0 <= index <= evidence.spec_findings().len(),
        decreases evidence.spec_findings().len() - index,
    {
        let finding = evidence.findings()[index];
        if finding.id() == waiver.finding_id()
            && finding.binding().is_current_for(candidate, evaluated_at)
            && matches!(finding.disposition(), FindingDisposition::WaiverRequested)
            && !finding.release_blocking()
            && waiver.authority() != finding.reporter()
        {
            return true;
        }
        index += 1;
    }
    false
}

} // verus!
