//! One exact release-evidence requirement assessment.

use crate::{EvidenceAssessment, EvidenceRequirement, ReleaseCandidate, ReleaseEvidence};
use peritus_types::Sha256Digest;

const fn finish_evidence(
    requirement: EvidenceRequirement,
    counts: (u16, u16, u16, u16, u16, u16),
    conflicting: bool,
    aggregate_digest: Sha256Digest,
) -> EvidenceAssessment {
    let satisfied = counts.0 > 0
        && counts.1 == 0
        && counts.2 == 0
        && counts.3 == 0
        && counts.4 == 0
        && counts.5 == 0
        && !conflicting;

    EvidenceAssessment::new(
        requirement,
        satisfied,
        counts.0,
        counts.1,
        counts.2,
        counts.3,
        counts.4,
        counts.5,
        conflicting,
        aggregate_digest,
    )
}
const fn record_digest(
    first_digest: &mut Option<Sha256Digest>,
    conflicting: &mut bool,
    aggregate: &mut [u8; 32],
    digest: Sha256Digest,
) {
    if let Some(previous) = *first_digest {
        if !crate::candidate::equality::digests_equal(previous, digest) {
            *conflicting = true;
        }
    } else {
        *first_digest = Some(digest);
    }
    super::super::xor_digest(aggregate, digest);
}
const fn finish_reduction(
    requirement: EvidenceRequirement,
    _context: (&ReleaseCandidate, u64, &ReleaseEvidence, Option<Sha256Digest>),
    counts: (u16, u16, u16, u16, u16, u16),
    conflicting: bool,
    aggregate: [u8; 32],
) -> EvidenceAssessment {
    let aggregate_digest = Sha256Digest::new(aggregate);

    finish_evidence(requirement, counts, conflicting, aggregate_digest)
}
#[allow(
    clippy::large_types_passed_by_value,
    reason = "exact candidate identity is a Copy policy value"
)]
pub(super) fn assess_evidence(
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> EvidenceAssessment {
    let mut contributing_count = 0u16;
    let mut stale_count = 0u16;
    let mut mismatched_count = 0u16;
    let mut wrong_source_count = 0u16;
    let mut unreviewed_count = 0u16;
    let mut unsigned_count = 0u16;
    let mut conflicting = false;
    let mut first_digest = None::<Sha256Digest>;
    let mut aggregate = [0u8; 32];
    let mut index = 0;
    while index < evidence.observations().len() {
        let observation = evidence.observations()[index];
        if crate::catalog::requirements_equal(observation.requirement(), requirement) {
            let binding = observation.binding();
            if binding.is_mismatched(candidate) {
                super::super::increment(&mut mismatched_count);
            } else if binding.is_stale_at(candidate, evaluated_at) {
                super::super::increment(&mut stale_count);
            } else if !crate::catalog::source_kinds_equal(
                observation.source_kind(),
                requirement.source_kind(),
            ) {
                super::super::increment(&mut wrong_source_count);
            } else {
                if !observation.reviewed() {
                    super::super::increment(&mut unreviewed_count);
                }
                if !observation.signed() {
                    super::super::increment(&mut unsigned_count);
                }
                if observation.reviewed() && observation.signed() {
                    super::super::increment(&mut contributing_count);
                    record_digest(
                        &mut first_digest,
                        &mut conflicting,
                        &mut aggregate,
                        observation.artifact_digest(),
                    );
                }
            }
        }
        index += 1;
    }
    finish_reduction(
        requirement,
        (&candidate, evaluated_at, evidence, first_digest),
        (
            contributing_count,
            stale_count,
            mismatched_count,
            wrong_source_count,
            unreviewed_count,
            unsigned_count,
        ),
        conflicting,
        aggregate,
    )
}
