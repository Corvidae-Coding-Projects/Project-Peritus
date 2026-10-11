use super::lookup;
use crate::{FindingAssessment, FindingDisposition, ReleaseCandidate, ReleaseEvidence};

#[allow(
    clippy::large_types_passed_by_value,
    clippy::too_many_lines,
    reason = "one auditable phase keeps finding and waiver interactions explicit"
)]
pub(in crate::evaluator) fn assess(
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
    while right < evidence.findings().len() {
        let finding = evidence.findings()[right];
        if finding.binding().is_mismatched(candidate) {
            super::super::increment(&mut mismatched_count);
        } else if finding.binding().is_stale_at(candidate, evaluated_at) {
            super::super::increment(&mut stale_count);
        } else {
            let resolved = matches!(finding.disposition(), FindingDisposition::Resolved);
            if finding.release_blocking() && !resolved {
                super::super::increment(&mut release_blocking_count);
            }
            match finding.disposition() {
                FindingDisposition::Open => super::super::increment(&mut open_count),
                FindingDisposition::Resolved => {}
                FindingDisposition::Ignored => super::super::increment(&mut ignored_count),
                FindingDisposition::Quarantined => super::super::increment(&mut quarantined_count),
                FindingDisposition::WaiverRequested => {
                    if finding.release_blocking()
                        || !lookup::has_valid_waiver(finding, candidate, evaluated_at, evidence)
                    {
                        super::super::increment(&mut open_count);
                    }
                }
            }
            let mut left = 0;
            while left < right {
                let previous = evidence.findings()[left];
                if previous.binding().is_current_for(candidate, evaluated_at)
                    && crate::identity::finding_ids_equal(previous.id(), finding.id())
                    && !crate::review::findings_equal(&previous, &finding)
                {
                    conflicting_finding = true;
                }
                left += 1;
            }
        }
        right += 1;
    }
    let mut waiver_index = 0;
    while waiver_index < evidence.waivers().len() {
        let waiver = evidence.waivers()[waiver_index];
        if waiver.binding().is_mismatched(candidate) {
            super::super::increment(&mut mismatched_count);
            super::super::increment(&mut invalid_waiver_count);
        } else if waiver.binding().is_stale_at(candidate, evaluated_at) {
            super::super::increment(&mut stale_count);
            super::super::increment(&mut invalid_waiver_count);
        } else if !waiver.approved()
            || !lookup::waiver_targets_eligible_finding(waiver, candidate, evaluated_at, evidence)
        {
            super::super::increment(&mut invalid_waiver_count);
        }
        let mut earlier_waiver = 0;
        while earlier_waiver < waiver_index {
            let previous = evidence.waivers()[earlier_waiver];
            if crate::identity::finding_ids_equal(previous.finding_id(), waiver.finding_id())
                && previous.is_current_for(candidate, evaluated_at)
                && waiver.is_current_for(candidate, evaluated_at)
                && !crate::review::waivers_equal(&previous, &waiver)
            {
                super::super::increment(&mut invalid_waiver_count);
            }
            earlier_waiver += 1;
        }
        waiver_index += 1;
    }
    let mut finding_index = 0;
    while finding_index < evidence.findings().len() {
        let finding = evidence.findings()[finding_index];
        if finding.binding().is_current_for(candidate, evaluated_at)
            && matches!(finding.disposition(), FindingDisposition::WaiverRequested)
            && !finding.release_blocking()
            && !lookup::has_valid_waiver(finding, candidate, evaluated_at, evidence)
        {
            super::super::increment(&mut invalid_waiver_count);
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
    let assessment = FindingAssessment::new(
        satisfied,
        stale_count,
        mismatched_count,
        open_count,
        release_blocking_count,
        ignored_count,
        quarantined_count,
        invalid_waiver_count,
        conflicting_finding,
    );
    assessment
}
