//! One exact release-evidence requirement assessment.

use crate::{EvidenceAssessment, EvidenceRequirement, ReleaseCandidate, ReleaseEvidence};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Exact stored fields produced by reducing all supplied observations for one requirement.
pub open spec fn assessment_matches_reduction(
    assessment: &EvidenceAssessment,
    evidence: &ReleaseEvidence,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    let state = super::model::through(
        evidence.spec_observations(),
        requirement,
        candidate,
        evaluated_at,
        evidence.spec_observations().len() as nat,
    );
    &&& assessment.spec_requirement() == requirement
    &&& assessment.spec_is_satisfied() == super::model::state_satisfied(state)
    &&& assessment.spec_contributing_count() == state.contributing_count
    &&& assessment.spec_stale_count() == state.stale_count
    &&& assessment.spec_mismatched_count() == state.mismatched_count
    &&& assessment.spec_wrong_source_count() == state.wrong_source_count
    &&& assessment.spec_unreviewed_count() == state.unreviewed_count
    &&& assessment.spec_unsigned_count() == state.unsigned_count
    &&& assessment.spec_is_conflicting() == state.conflicting
    &&& assessment.spec_contributing_digest().spec_bytes()@ == state.aggregate_digest
}

proof fn matched_observation_is_admitted(
    evidence: &ReleaseEvidence,
    index: int,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
)
    requires
        0 <= index < evidence.spec_observations().len(),
        evidence.spec_observations()[index].spec_requirement() == requirement,
        !evidence.spec_observations()[index].spec_binding().spec_is_mismatched(candidate),
        !evidence.spec_observations()[index]
            .spec_binding()
            .spec_is_stale_at(candidate, evaluated_at),
        evidence.spec_observations()[index].spec_source_kind()
            == requirement.spec_source_kind(),
        evidence.spec_observations()[index].spec_reviewed(),
        evidence.spec_observations()[index].spec_signed(),
    ensures super::super::evidence_requirement_admitted(evidence, requirement, candidate, evaluated_at),
{
    let observation = evidence.spec_observations()[index];
    reveal(crate::EvidenceBinding::spec_is_current_for);
    reveal(crate::EvidenceBinding::spec_is_mismatched);
    reveal(crate::EvidenceBinding::spec_is_stale_at);
    assert(observation.spec_binding().spec_is_current_for(candidate, evaluated_at));
    reveal(crate::EvidenceObservation::spec_contributes_to);
    assert(observation.spec_contributes_to(requirement, candidate, evaluated_at));
    reveal(super::super::evidence_requirement_admitted);
    assert(exists |witness: int| witness == index
        && 0 <= witness < evidence.spec_observations().len()
        && #[trigger] evidence.spec_observations()[witness].spec_contributes_to(
            requirement,
            candidate,
            evaluated_at,
    ));
}

closed spec fn evidence_counts_satisfied(
    counts: (u16, u16, u16, u16, u16, u16),
    conflicting: bool,
) -> bool {
    counts.0 > 0
        && counts.1 == 0
        && counts.2 == 0
        && counts.3 == 0
        && counts.4 == 0
        && counts.5 == 0
        && !conflicting
}

const fn finish_evidence(
    requirement: EvidenceRequirement,
    counts: (u16, u16, u16, u16, u16, u16),
    conflicting: bool,
    aggregate_digest: Sha256Digest,
) -> (assessment: EvidenceAssessment)
    ensures
        assessment.spec_requirement() == requirement,
        assessment.spec_is_satisfied() == evidence_counts_satisfied(counts, conflicting),
        assessment.spec_contributing_count() == counts.0,
        assessment.spec_stale_count() == counts.1,
        assessment.spec_mismatched_count() == counts.2,
        assessment.spec_wrong_source_count() == counts.3,
        assessment.spec_unreviewed_count() == counts.4,
        assessment.spec_unsigned_count() == counts.5,
        assessment.spec_is_conflicting() == conflicting,
        assessment.spec_contributing_digest() == aggregate_digest,
        assessment.spec_diagnostics_clear() == evidence_counts_satisfied(counts, conflicting),
{
    let satisfied = counts.0 > 0
        && counts.1 == 0
        && counts.2 == 0
        && counts.3 == 0
        && counts.4 == 0
        && counts.5 == 0
        && !conflicting;
    let assessment = EvidenceAssessment::new(
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
    );
    proof {
        reveal(evidence_counts_satisfied);
        reveal(EvidenceAssessment::spec_diagnostics_clear);
    }
    assessment
}

const fn record_digest(
    first_digest: &mut Option<Sha256Digest>,
    conflicting: &mut bool,
    aggregate: &mut [u8; 32],
    digest: Sha256Digest,
)
    ensures
        *final(first_digest) == match *old(first_digest) {
            Some(previous) => Some(previous),
            None => Some(digest),
        },
        *final(conflicting) == (*old(conflicting) || match *old(first_digest) {
            Some(previous) => !crate::candidate::digest_matches(previous, digest),
            None => false,
        }),
        final(aggregate)@ == super::super::xor_digest_bytes(old(aggregate)@, digest),
{
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
) -> (assessment: EvidenceAssessment)
    requires
        super::model::corresponds(
            super::model::through(
                _context.2.spec_observations(), requirement, *_context.0, _context.1,
                _context.2.spec_observations().len() as nat),
            counts.0, counts.1, counts.2, counts.3, counts.4, counts.5,
            conflicting, _context.3, aggregate@,
        ),
        counts.0 > 0 ==> super::super::evidence_requirement_admitted(
            _context.2, requirement, *_context.0, _context.1),
    ensures
        assessment_matches_reduction(
            &assessment, _context.2, requirement, *_context.0, _context.1),
        assessment.spec_is_satisfied()
            == super::requirement_satisfied(_context.2, requirement, *_context.0, _context.1),
        assessment.spec_diagnostics_clear()
            == super::requirement_satisfied(_context.2, requirement, *_context.0, _context.1),
        assessment.spec_is_satisfied() ==> super::super::evidence_requirement_admitted(
            _context.2, requirement, *_context.0, _context.1),
{
    let aggregate_digest = Sha256Digest::new(aggregate);
    let assessment = finish_evidence(requirement, counts, conflicting, aggregate_digest);
    proof {
        super::declarative::reduction_matches_inputs(
            _context.2, requirement, *_context.0, _context.1);
        reveal(super::requirement_satisfied);
        reveal(super::model::requirement_satisfied);
        reveal(super::model::state_satisfied);
        reveal(super::model::corresponds);
        assert(aggregate_digest.spec_bytes()@ == aggregate@);
        reveal(evidence_counts_satisfied);
        reveal(assessment_matches_reduction);
    }
    assessment
}

#[allow(clippy::large_types_passed_by_value, reason = "exact candidate identity is a Copy policy value")]
pub(super) fn assess_evidence(
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> (assessment: EvidenceAssessment)
    ensures
        assessment_matches_reduction(
            &assessment,
            evidence,
            requirement,
            candidate,
            evaluated_at,
        ),
        assessment.spec_is_satisfied()
            == super::requirement_satisfied(evidence, requirement, candidate, evaluated_at),
        assessment.spec_diagnostics_clear()
            == super::requirement_satisfied(evidence, requirement, candidate, evaluated_at),
        assessment.spec_is_satisfied() ==>
            super::super::evidence_requirement_admitted(evidence, requirement, candidate, evaluated_at),
{
    let mut contributing_count = 0u16;
    let mut stale_count = 0u16;
    let mut mismatched_count = 0u16;
    let mut wrong_source_count = 0u16;
    let mut unreviewed_count = 0u16;
    let mut unsigned_count = 0u16;
    let mut conflicting = false;
    let mut first_digest = None::<Sha256Digest>;
    let mut aggregate = [0u8; 32];
    proof {
        assert(aggregate@ =~= Seq::new(32, |index: int| 0u8));
    }
    let mut index = 0;
    while index < evidence.observations().len()
        invariant
            0 <= index <= evidence.spec_observations().len(),
            super::model::corresponds(
                super::model::through(
                    evidence.spec_observations(),
                    requirement,
                    candidate,
                    evaluated_at,
                    index as nat,
                ),
                contributing_count,
                stale_count,
                mismatched_count,
                wrong_source_count,
                unreviewed_count,
                unsigned_count,
                conflicting,
                first_digest,
                aggregate@,
            ),
            aggregate@.len() == 32,
            contributing_count > 0 ==>
                super::super::evidence_requirement_admitted(evidence, requirement, candidate, evaluated_at),
        decreases evidence.spec_observations().len() - index,
    {
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
                if !observation.reviewed() { super::super::increment(&mut unreviewed_count); }
                if !observation.signed() { super::super::increment(&mut unsigned_count); }
                if observation.reviewed() && observation.signed() {
                    super::super::increment(&mut contributing_count);
                    proof {
                        matched_observation_is_admitted(
                            evidence,
                            index as int,
                            requirement,
                            candidate,
                            evaluated_at,
                        );
                    }
                    record_digest(
                        &mut first_digest,
                        &mut conflicting,
                        &mut aggregate,
                        observation.artifact_digest(),
                    );
                }
            }
        }
        proof {
            reveal(super::model::through);
            reveal(super::model::step);
            reveal(super::model::corresponds);
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



} // verus!
