#[cfg(verus_only)]
use super::{assessment_matches_reduction, declarative, findings_satisfied, model};
use super::lookup;
use crate::{FindingAssessment, FindingDisposition, ReleaseCandidate, ReleaseEvidence};
use vstd::prelude::*;

verus! {
#[allow(clippy::large_types_passed_by_value, clippy::too_many_lines,
    reason = "one auditable phase keeps finding and waiver interactions explicit")]
pub(in crate::evaluator) fn assess(
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
    ) -> (assessment: FindingAssessment)
    ensures
        assessment_matches_reduction(assessment, evidence, candidate, evaluated_at),
        assessment.spec_is_satisfied()
            == findings_satisfied(evidence, candidate, evaluated_at),
        assessment.spec_diagnostics_clear()
            == findings_satisfied(evidence, candidate, evaluated_at),
{
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
        invariant
            0 <= right <= evidence.spec_findings().len(),
            model::corresponds(
                model::findings_through(evidence, candidate, evaluated_at, right as nat),
                stale_count,
                mismatched_count,
                open_count,
                release_blocking_count,
                ignored_count,
                quarantined_count,
                invalid_waiver_count,
                conflicting_finding,
            ),
            (stale_count == 0
                && mismatched_count == 0
                && open_count == 0
                && release_blocking_count == 0
                && ignored_count == 0
                && quarantined_count == 0
                && invalid_waiver_count == 0
                && !conflicting_finding)
                == model::findings_clear_through(
                    evidence,
                    candidate,
                    evaluated_at,
                    right as nat,
                ),
        decreases evidence.spec_findings().len() - right,
    {
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
            while left < right
                invariant
                    0 <= left <= right,
                    right < evidence.spec_findings().len(),
                    model::corresponds(
                        model::finding_pairs_through(
                            model::finding_base_step(
                                model::findings_through(
                                    evidence, candidate, evaluated_at, right as nat),
                                evidence,
                                finding,
                                candidate,
                                evaluated_at,
                            ),
                            evidence.spec_findings(),
                            finding,
                            candidate,
                            evaluated_at,
                            left as nat,
                        ),
                        stale_count,
                        mismatched_count,
                        open_count,
                        release_blocking_count,
                        ignored_count,
                        quarantined_count,
                        invalid_waiver_count,
                        conflicting_finding,
                    ),
                    (stale_count == 0
                        && mismatched_count == 0
                        && open_count == 0
                        && release_blocking_count == 0
                        && ignored_count == 0
                        && quarantined_count == 0
                        && invalid_waiver_count == 0
                        && !conflicting_finding)
                        == (model::findings_clear_through(
                            evidence,
                            candidate,
                            evaluated_at,
                            right as nat,
                        ) && model::finding_base_clear(
                            evidence,
                            finding,
                            candidate,
                            evaluated_at,
                        ) && model::finding_pairs_clear_through(
                            evidence.spec_findings(),
                            finding,
                            candidate,
                            evaluated_at,
                            left as nat,
                        )),
                decreases right - left,
            {
                let previous = evidence.findings()[left];
                if previous.binding().is_current_for(candidate, evaluated_at)
                    && crate::identity::finding_ids_equal(previous.id(), finding.id())
                    && !crate::review::findings_equal(&previous, &finding)
                {
                    conflicting_finding = true;
                }
                proof {
                    reveal(model::finding_pairs_through);
                    reveal(model::finding_pair_conflicts);
                    reveal(model::corresponds);
                    reveal(model::finding_pairs_clear_through);
                    reveal(model::finding_pair_clear);
                }
                left += 1;
            }
        }
        proof {
            reveal(model::findings_through);
            reveal(model::finding_step);
            reveal(model::finding_base_step);
            reveal(model::corresponds);
            reveal(model::findings_clear_through);
            reveal(model::finding_base_clear);
        }
        right += 1;
    }

    let mut waiver_index = 0;
    while waiver_index < evidence.waivers().len()
        invariant
            0 <= waiver_index <= evidence.spec_waivers().len(),
            model::corresponds(
                model::waivers_through(evidence, candidate, evaluated_at, waiver_index as nat),
                stale_count,
                mismatched_count,
                open_count,
                release_blocking_count,
                ignored_count,
                quarantined_count,
                invalid_waiver_count,
                conflicting_finding,
            ),
            (stale_count == 0
                && mismatched_count == 0
                && open_count == 0
                && release_blocking_count == 0
                && ignored_count == 0
                && quarantined_count == 0
                && invalid_waiver_count == 0
                && !conflicting_finding)
                == (model::findings_clear_through(
                    evidence,
                    candidate,
                    evaluated_at,
                    evidence.spec_findings().len() as nat,
                ) && model::waivers_clear_through(
                    evidence,
                    candidate,
                    evaluated_at,
                    waiver_index as nat,
                )),
        decreases evidence.spec_waivers().len() - waiver_index,
    {
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
        while earlier_waiver < waiver_index
            invariant
                0 <= earlier_waiver <= waiver_index,
                waiver_index < evidence.spec_waivers().len(),
                model::corresponds(
                    model::waiver_pairs_through(
                        model::waiver_base_step(
                            model::waivers_through(
                                evidence, candidate, evaluated_at, waiver_index as nat),
                            evidence,
                            waiver,
                            candidate,
                            evaluated_at,
                        ),
                        evidence.spec_waivers(),
                        waiver,
                        candidate,
                        evaluated_at,
                        earlier_waiver as nat,
                    ),
                    stale_count,
                    mismatched_count,
                    open_count,
                    release_blocking_count,
                    ignored_count,
                    quarantined_count,
                    invalid_waiver_count,
                    conflicting_finding,
                ),
                (stale_count == 0
                    && mismatched_count == 0
                    && open_count == 0
                    && release_blocking_count == 0
                    && ignored_count == 0
                    && quarantined_count == 0
                    && invalid_waiver_count == 0
                    && !conflicting_finding)
                    == (model::findings_clear_through(
                        evidence,
                        candidate,
                        evaluated_at,
                        evidence.spec_findings().len() as nat,
                    ) && model::waivers_clear_through(
                        evidence,
                        candidate,
                        evaluated_at,
                        waiver_index as nat,
                    ) && model::waiver_base_clear(
                        evidence,
                        waiver,
                        candidate,
                        evaluated_at,
                    ) && model::waiver_pairs_clear_through(
                        evidence.spec_waivers(),
                        waiver,
                        candidate,
                        evaluated_at,
                        earlier_waiver as nat,
                    )),
            decreases waiver_index - earlier_waiver,
        {
            let previous = evidence.waivers()[earlier_waiver];
            if crate::identity::finding_ids_equal(previous.finding_id(), waiver.finding_id())
                && previous.is_current_for(candidate, evaluated_at)
                && waiver.is_current_for(candidate, evaluated_at)
                && !crate::review::waivers_equal(&previous, &waiver)
            {
                super::super::increment(&mut invalid_waiver_count);
            }
            proof {
                reveal(model::waiver_pairs_through);
                reveal(model::waiver_pair_invalid);
                reveal(model::corresponds);
                reveal(model::waiver_pairs_clear_through);
                reveal(model::waiver_pair_clear);
            }
            earlier_waiver += 1;
        }
        proof {
            reveal(model::waivers_through);
            reveal(model::waiver_step);
            reveal(model::waiver_base_step);
            reveal(model::corresponds);
            reveal(model::waivers_clear_through);
            reveal(model::waiver_base_clear);
        }
        waiver_index += 1;
    }

    let mut finding_index = 0;
    while finding_index < evidence.findings().len()
        invariant
            0 <= finding_index <= evidence.spec_findings().len(),
            model::corresponds(
                model::requests_through(evidence, candidate, evaluated_at, finding_index as nat),
                stale_count,
                mismatched_count,
                open_count,
                release_blocking_count,
                ignored_count,
                quarantined_count,
                invalid_waiver_count,
                conflicting_finding,
            ),
            (stale_count == 0
                && mismatched_count == 0
                && open_count == 0
                && release_blocking_count == 0
                && ignored_count == 0
                && quarantined_count == 0
                && invalid_waiver_count == 0
                && !conflicting_finding)
                == (model::findings_clear_through(
                    evidence,
                    candidate,
                    evaluated_at,
                    evidence.spec_findings().len() as nat,
                ) && model::waivers_clear_through(
                    evidence,
                    candidate,
                    evaluated_at,
                    evidence.spec_waivers().len() as nat,
                ) && model::requests_clear_through(
                    evidence,
                    candidate,
                    evaluated_at,
                    finding_index as nat,
                )),
        decreases evidence.spec_findings().len() - finding_index,
    {
        let finding = evidence.findings()[finding_index];
        if finding.binding().is_current_for(candidate, evaluated_at)
            && matches!(finding.disposition(), FindingDisposition::WaiverRequested)
            && !finding.release_blocking()
            && !lookup::has_valid_waiver(finding, candidate, evaluated_at, evidence)
        {
            super::super::increment(&mut invalid_waiver_count);
        }
        proof {
            reveal(model::requests_through);
            reveal(model::request_step);
            reveal(model::corresponds);
            reveal(model::requests_clear_through);
            reveal(model::request_clear);
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
    proof {
        declarative::reduction_matches_inputs(evidence, candidate, evaluated_at);
        reveal(assessment_matches_reduction);
        reveal(model::final_state);
        reveal(model::state_satisfied);
        reveal(model::corresponds);
        reveal(findings_satisfied);
        reveal(model::findings_satisfied);
        reveal(FindingAssessment::spec_diagnostics_clear);
    }
    assessment
}

} // verus!
