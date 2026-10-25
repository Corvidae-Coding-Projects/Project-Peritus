//! H0-H3 qualification reduction.

use crate::{
    QualificationAssessment, QualificationSlice, QualificationVerdict, ReleaseCandidate,
    ReleaseEvidence,
};
use peritus_types::Sha256Digest;

pub(super) fn assess_all(
    candidate: &ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> [QualificationAssessment; 4] {
    [
        assess(QualificationSlice::H0Security, *candidate, evaluated_at, evidence),
        assess(QualificationSlice::H1Resilience, *candidate, evaluated_at, evidence),
        assess(QualificationSlice::H2Platform, *candidate, evaluated_at, evidence),
        assess(QualificationSlice::H3Performance, *candidate, evaluated_at, evidence),
    ]
}
const fn slices_equal(left: QualificationSlice, right: QualificationSlice) -> bool {
    matches!(
        (left, right),
        (QualificationSlice::H0Security, QualificationSlice::H0Security)
            | (QualificationSlice::H1Resilience, QualificationSlice::H1Resilience)
            | (QualificationSlice::H2Platform, QualificationSlice::H2Platform)
            | (QualificationSlice::H3Performance, QualificationSlice::H3Performance)
    )
}
const fn verdicts_equal(left: QualificationVerdict, right: QualificationVerdict) -> bool {
    matches!(
        (left, right),
        (QualificationVerdict::Ready, QualificationVerdict::Ready)
            | (
                QualificationVerdict::NotReadyForProduction,
                QualificationVerdict::NotReadyForProduction
            )
    )
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
) -> QualificationAssessment {
    let satisfied = counts.0 > 0
        && counts.1 == 0
        && counts.2 == 0
        && counts.3 == 0
        && counts.4 == 0
        && !conflicting;

    QualificationAssessment::new(
        slice,
        satisfied,
        counts.0,
        counts.1,
        counts.2,
        counts.3,
        counts.4,
        conflicting,
        Sha256Digest::new(aggregate),
    )
}
#[allow(
    clippy::large_types_passed_by_value,
    reason = "exact candidate identity is a Copy policy value"
)]
pub(super) fn assess(
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> QualificationAssessment {
    let mut ready_count = 0u16;
    let mut stale_count = 0u16;
    let mut mismatched_count = 0u16;
    let mut unreviewed_count = 0u16;
    let mut not_ready_count = 0u16;
    let mut conflicting = false;
    let mut first_report = None::<Sha256Digest>;
    let mut first_verdict = None::<QualificationVerdict>;
    let mut aggregate = [0u8; 32];
    let mut index = 0;
    while index < evidence.qualifications().len() {
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
                    if !verdicts_equal(previous, observation.verdict()) {
                        conflicting = true;
                    }
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
