//! H0-H3 qualification reduction.

use crate::{
    QualificationAssessment, QualificationSlice, QualificationVerdict, ReleaseCandidate,
    ReleaseEvidence,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

#[allow(clippy::large_types_passed_by_value, reason = "exact candidate identity is a Copy policy value")]
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
    while index < evidence.qualifications().len()
        invariant 0 <= index <= evidence.spec_qualifications().len(),
        decreases evidence.spec_qualifications().len() - index,
    {
        let observation = evidence.qualifications()[index];
        if observation.slice() == slice {
            if observation.binding().is_mismatched(candidate) {
                super::increment(&mut mismatched_count);
            } else if observation.binding().is_stale_at(candidate, evaluated_at) {
                super::increment(&mut stale_count);
            } else if !observation.reviewed() {
                super::increment(&mut unreviewed_count);
            } else {
                if let Some(previous) = first_report {
                    if previous != observation.report_digest() { conflicting = true; }
                } else {
                    first_report = Some(observation.report_digest());
                }
                if let Some(previous) = first_verdict {
                    if previous != observation.verdict() { conflicting = true; }
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
    let satisfied = ready_count > 0
        && stale_count == 0
        && mismatched_count == 0
        && unreviewed_count == 0
        && not_ready_count == 0
        && !conflicting;
    QualificationAssessment::new(
        slice,
        satisfied,
        ready_count,
        stale_count,
        mismatched_count,
        unreviewed_count,
        not_ready_count,
        conflicting,
        Sha256Digest::new(aggregate),
    )
}

} // verus!
