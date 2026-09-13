//! H0-H3 qualification reduction.

#[cfg(verus_only)]
pub mod model;
#[cfg(verus_only)]
mod declarative;

use crate::{
    QualificationAssessment, QualificationSlice, QualificationVerdict, ReleaseCandidate,
    ReleaseEvidence,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Exact stored fields produced by reducing all supplied reports for one H-slice.
pub open spec fn assessment_matches_reduction(
    assessment: &QualificationAssessment,
    evidence: &ReleaseEvidence,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    let state = model::through(
        evidence.spec_qualifications(),
        slice,
        candidate,
        evaluated_at,
        evidence.spec_qualifications().len() as nat,
    );
    &&& assessment.spec_slice() == slice
    &&& assessment.spec_is_satisfied() == model::state_satisfied(state)
    &&& assessment.spec_ready_count() == state.ready_count
    &&& assessment.spec_stale_count() == state.stale_count
    &&& assessment.spec_mismatched_count() == state.mismatched_count
    &&& assessment.spec_unreviewed_count() == state.unreviewed_count
    &&& assessment.spec_not_ready_count() == state.not_ready_count
    &&& assessment.spec_is_conflicting() == state.conflicting
    &&& assessment.spec_report_digest().spec_bytes()@ == state.aggregate_digest
}

/// Exact stable H0-H3 slot identities and their complete finite report reductions.
pub open spec fn assessments_match_inputs(
    assessments: Seq<QualificationAssessment>,
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    assessments.len() == 4 && assessment_matches_reduction(
        &assessments[0], evidence, QualificationSlice::H0Security, candidate, evaluated_at,
    ) && assessment_matches_reduction(
        &assessments[1], evidence, QualificationSlice::H1Resilience, candidate, evaluated_at,
    ) && assessment_matches_reduction(
        &assessments[2], evidence, QualificationSlice::H2Platform, candidate, evaluated_at,
    ) && assessment_matches_reduction(
        &assessments[3], evidence, QualificationSlice::H3Performance, candidate, evaluated_at,
    )
}

pub open spec fn qualification_satisfied(
    evidence: &ReleaseEvidence,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    declarative::qualification_inputs_ready(
        evidence.spec_qualifications(), slice, candidate, evaluated_at,
    )
}

pub open spec fn all_qualifications_satisfied(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    qualification_satisfied(
        evidence,
        QualificationSlice::H0Security,
        candidate,
        evaluated_at,
    ) && qualification_satisfied(
        evidence,
        QualificationSlice::H1Resilience,
        candidate,
        evaluated_at,
    ) && qualification_satisfied(
        evidence,
        QualificationSlice::H2Platform,
        candidate,
        evaluated_at,
    ) && qualification_satisfied(
        evidence,
        QualificationSlice::H3Performance,
        candidate,
        evaluated_at,
    )
}

pub(super) fn assess_all(
    candidate: &ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> (assessments: [QualificationAssessment; 4])
    ensures
        assessments_match_inputs(assessments@, evidence, *candidate, evaluated_at),
        crate::decision::spec_qualifications_complete(&assessments)
            == all_qualifications_satisfied(evidence, *candidate, evaluated_at),
        super::diagnostics::qualifications_clear(&assessments)
            == all_qualifications_satisfied(evidence, *candidate, evaluated_at),
{
    let assessments = [
        assess(QualificationSlice::H0Security, *candidate, evaluated_at, evidence),
        assess(QualificationSlice::H1Resilience, *candidate, evaluated_at, evidence),
        assess(QualificationSlice::H2Platform, *candidate, evaluated_at, evidence),
        assess(QualificationSlice::H3Performance, *candidate, evaluated_at, evidence),
    ];
    proof {
        reveal(crate::decision::spec_qualifications_complete);
        reveal(all_qualifications_satisfied);
        reveal(assessments_match_inputs);
        reveal(assessment_matches_reduction);
        reveal(super::diagnostics::qualifications_clear);
        reveal_with_fuel(super::diagnostics::qualifications_clear_through, 5);
    }
    assessments
}

const fn slices_equal(left: QualificationSlice, right: QualificationSlice) -> (equal: bool)
    ensures equal == (left == right),
{
    matches!((left, right),
        (QualificationSlice::H0Security, QualificationSlice::H0Security)
            | (QualificationSlice::H1Resilience, QualificationSlice::H1Resilience)
            | (QualificationSlice::H2Platform, QualificationSlice::H2Platform)
            | (QualificationSlice::H3Performance, QualificationSlice::H3Performance))
}

const fn verdicts_equal(left: QualificationVerdict, right: QualificationVerdict) -> (equal: bool)
    ensures equal == (left == right),
{
    matches!((left, right),
        (QualificationVerdict::Ready, QualificationVerdict::Ready)
            | (QualificationVerdict::NotReadyForProduction,
                QualificationVerdict::NotReadyForProduction))
}

const fn finish_reduction(
    slice: QualificationSlice,
    _context: (
        &ReleaseCandidate,
        u64,
        &ReleaseEvidence,
        Option<Sha256Digest>,
        Option<QualificationVerdict>,
    ),
    counts: (u16, u16, u16, u16, u16),
    conflicting: bool,
    aggregate: [u8; 32],
) -> (assessment: QualificationAssessment)
    requires model::corresponds(
        model::through(
            _context.2.spec_qualifications(), slice, *_context.0, _context.1,
            _context.2.spec_qualifications().len() as nat),
        counts.0, counts.1, counts.2, counts.3, counts.4, conflicting,
        _context.3, _context.4, aggregate@,
    ),
    ensures
        assessment_matches_reduction(
            &assessment, _context.2, slice, *_context.0, _context.1),
        assessment.spec_is_satisfied()
            == qualification_satisfied(_context.2, slice, *_context.0, _context.1),
        assessment.spec_diagnostics_clear()
            == qualification_satisfied(_context.2, slice, *_context.0, _context.1),
{
    let satisfied = counts.0 > 0
        && counts.1 == 0
        && counts.2 == 0
        && counts.3 == 0
        && counts.4 == 0
        && !conflicting;
    let assessment = QualificationAssessment::new(
        slice,
        satisfied,
        counts.0,
        counts.1,
        counts.2,
        counts.3,
        counts.4,
        conflicting,
        Sha256Digest::new(aggregate),
    );
    proof {
        declarative::reduction_matches_inputs(
            _context.2, slice, *_context.0, _context.1);
        assert(assessment.spec_report_digest().spec_bytes()@ == aggregate@);
        reveal(qualification_satisfied);
        reveal(model::qualification_satisfied);
        reveal(model::state_satisfied);
        reveal(model::corresponds);
        reveal(QualificationAssessment::spec_diagnostics_clear);
        reveal(assessment_matches_reduction);
    }
    assessment
}

#[allow(clippy::large_types_passed_by_value, reason = "exact candidate identity is a Copy policy value")]
pub(super) fn assess(
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> (assessment: QualificationAssessment)
    ensures
        assessment_matches_reduction(&assessment, evidence, slice, candidate, evaluated_at),
        assessment.spec_is_satisfied()
            == qualification_satisfied(evidence, slice, candidate, evaluated_at),
        assessment.spec_diagnostics_clear()
            == qualification_satisfied(evidence, slice, candidate, evaluated_at),
{
    let mut ready_count = 0u16;
    let mut stale_count = 0u16;
    let mut mismatched_count = 0u16;
    let mut unreviewed_count = 0u16;
    let mut not_ready_count = 0u16;
    let mut conflicting = false;
    let mut first_report = None::<Sha256Digest>;
    let mut first_verdict = None::<QualificationVerdict>;
    let mut aggregate = [0u8; 32];
    proof {
        assert(aggregate@ =~= Seq::new(32, |index: int| 0u8));
    }
    let mut index = 0;
    while index < evidence.qualifications().len()
        invariant
            0 <= index <= evidence.spec_qualifications().len(),
            model::corresponds(
                model::through(
                    evidence.spec_qualifications(),
                    slice,
                    candidate,
                    evaluated_at,
                    index as nat,
                ),
                ready_count,
                stale_count,
                mismatched_count,
                unreviewed_count,
                not_ready_count,
                conflicting,
                first_report,
                first_verdict,
                aggregate@,
            ),
            aggregate@.len() == 32,
        decreases evidence.spec_qualifications().len() - index,
    {
        let observation = evidence.qualifications()[index];
        if slices_equal(observation.slice(), slice) {
            if observation.binding().is_mismatched(candidate) {
                super::increment(&mut mismatched_count);
            } else if observation.binding().is_stale_at(candidate, evaluated_at) {
                super::increment(&mut stale_count);
            } else if !observation.reviewed() {
                super::increment(&mut unreviewed_count);
            } else {
                if let Some(previous) = first_report {
                    if !crate::candidate::equality::digests_equal(
                        previous,
                        observation.report_digest(),
                    ) {
                        conflicting = true;
                    }
                } else {
                    first_report = Some(observation.report_digest());
                }
                if let Some(previous) = first_verdict {
                    if !verdicts_equal(previous, observation.verdict()) { conflicting = true; }
                } else {
                    first_verdict = Some(observation.verdict());
                }
                match observation.verdict() {
                    QualificationVerdict::Ready => {
                        super::increment(&mut ready_count);
                        super::xor_digest(&mut aggregate, observation.report_digest());
                    }
                    QualificationVerdict::NotReadyForProduction => {
                        super::increment(&mut not_ready_count);
                    }
                }
            }
        }
        proof {
        reveal(model::through);
            reveal(model::step);
            reveal(model::corresponds);
        }
        index += 1;
    }
    finish_reduction(
        slice,
        (&candidate, evaluated_at, evidence, first_report, first_verdict),
        (ready_count, stale_count, mismatched_count, unreviewed_count, not_ready_count),
        conflicting,
        aggregate,
    )
}

} // verus!
