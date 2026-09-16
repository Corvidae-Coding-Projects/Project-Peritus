//! Actual-input specifications for requirement and criterion evaluation.

#[cfg(verus_only)]
use crate::{
    AcceptanceCriterion, CriterionObservation, IntegratedCandidate, RequirementObservation,
    SecurityEvidence, SecurityRequirement,
};
use vstd::prelude::*;

verus! {

pub open spec fn requirement_observation_matches(
    observation: RequirementObservation,
    target: SecurityRequirement,
    candidate: IntegratedCandidate,
) -> bool {
    observation.spec_requirement() == target
        && crate::binding::candidate_fresh(observation.spec_candidate(), candidate)
}

pub open spec fn criterion_observation_matches(
    observation: CriterionObservation,
    target: AcceptanceCriterion,
    candidate: IntegratedCandidate,
) -> bool {
    observation.spec_criterion() == target
        && crate::binding::candidate_fresh(observation.spec_candidate(), candidate)
}

pub open spec fn first_requirement_observation_at(
    values: Seq<RequirementObservation>,
    index: int,
    target: SecurityRequirement,
    candidate: IntegratedCandidate,
) -> bool {
    0 <= index < values.len()
        && requirement_observation_matches(values[index], target, candidate)
        && (forall |prior: int| 0 <= prior < index ==>
            !requirement_observation_matches(#[trigger] values[prior], target, candidate))
}

pub open spec fn first_criterion_observation_at(
    values: Seq<CriterionObservation>,
    index: int,
    target: AcceptanceCriterion,
    candidate: IntegratedCandidate,
) -> bool {
    0 <= index < values.len()
        && criterion_observation_matches(values[index], target, candidate)
        && (forall |prior: int| 0 <= prior < index ==>
            !criterion_observation_matches(#[trigger] values[prior], target, candidate))
}

pub open spec fn requirement_admitted(
    values: Seq<RequirementObservation>,
    target: SecurityRequirement,
    candidate: IntegratedCandidate,
) -> bool {
    exists |index: int| {
        let observation = #[trigger] values[index];
        &&& first_requirement_observation_at(values, index, target, candidate)
        &&& observation.spec_outcome().spec_is_passed()
        &&& crate::binding::digest_is_present(observation.spec_evidence_digest())
    }
}

pub open spec fn criterion_admitted(
    values: Seq<CriterionObservation>,
    target: AcceptanceCriterion,
    candidate: IntegratedCandidate,
) -> bool {
    exists |index: int| {
        let observation = #[trigger] values[index];
        &&& first_criterion_observation_at(values, index, target, candidate)
        &&& observation.spec_outcome().spec_is_passed()
        &&& crate::binding::digest_is_present(observation.spec_evidence_digest())
    }
}

pub open spec fn requirements_complete_through(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
    end: int,
) -> bool {
    forall |index: int| 0 <= index < end ==>
        #[trigger] requirement_admitted(
            evidence.spec_requirements(),
            SecurityRequirement::ALL[index],
            candidate,
        )
}

pub open spec fn criteria_complete_through(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
    end: int,
) -> bool {
    forall |index: int| 0 <= index < end ==>
        #[trigger] criterion_admitted(
            evidence.spec_criteria(),
            AcceptanceCriterion::ALL[index],
            candidate,
        )
}

/// Every canonical R-SEC requirement has an actual current passing observation and digest.
pub open spec fn requirements_complete(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> bool {
    requirements_complete_through(evidence, candidate, SecurityRequirement::ALL.len() as int)
}

/// Every canonical criterion has an actual current passing observation and digest.
pub open spec fn criteria_complete(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> bool {
    criteria_complete_through(evidence, candidate, AcceptanceCriterion::ALL.len() as int)
}

pub proof fn requirements_complete_step(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
    end: int,
)
    requires 0 <= end < SecurityRequirement::ALL.len(),
    ensures requirements_complete_through(evidence, candidate, end + 1) == (
        requirements_complete_through(evidence, candidate, end)
            && requirement_admitted(
                evidence.spec_requirements(),
                SecurityRequirement::ALL[end],
                candidate,
            )
    ),
{
    let current = requirement_admitted(
        evidence.spec_requirements(),
        SecurityRequirement::ALL[end],
        candidate,
    );
    if requirements_complete_through(evidence, candidate, end + 1) {
        assert(requirements_complete_through(evidence, candidate, end));
        assert(current);
    } else if requirements_complete_through(evidence, candidate, end) && current {
        assert(requirements_complete_through(evidence, candidate, end + 1)) by {
            assert forall |index: int| 0 <= index < end + 1 implies
                #[trigger] requirement_admitted(
                    evidence.spec_requirements(),
                    SecurityRequirement::ALL[index],
                    candidate,
                ) by {
                if index < end {
                    assert(requirement_admitted(
                        evidence.spec_requirements(),
                        SecurityRequirement::ALL[index],
                        candidate,
                    ));
                } else {
                    assert(index == end);
                }
            }
        };
    }
}

pub proof fn criteria_complete_step(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
    end: int,
)
    requires 0 <= end < AcceptanceCriterion::ALL.len(),
    ensures criteria_complete_through(evidence, candidate, end + 1) == (
        criteria_complete_through(evidence, candidate, end)
            && criterion_admitted(
                evidence.spec_criteria(),
                AcceptanceCriterion::ALL[end],
                candidate,
            )
    ),
{
    let current = criterion_admitted(
        evidence.spec_criteria(),
        AcceptanceCriterion::ALL[end],
        candidate,
    );
    if criteria_complete_through(evidence, candidate, end + 1) {
        assert(criteria_complete_through(evidence, candidate, end));
        assert(current);
    } else if criteria_complete_through(evidence, candidate, end) && current {
        assert(criteria_complete_through(evidence, candidate, end + 1)) by {
            assert forall |index: int| 0 <= index < end + 1 implies
                #[trigger] criterion_admitted(
                    evidence.spec_criteria(),
                    AcceptanceCriterion::ALL[index],
                    candidate,
                ) by {
                if index < end {
                    assert(criterion_admitted(
                        evidence.spec_criteria(),
                        AcceptanceCriterion::ALL[index],
                        candidate,
                    ));
                } else {
                    assert(index == end);
                }
            }
        };
    }
}

pub proof fn first_requirement_index_unique(
    values: Seq<RequirementObservation>,
    left: int,
    right: int,
    target: SecurityRequirement,
    candidate: IntegratedCandidate,
)
    requires
        first_requirement_observation_at(values, left, target, candidate),
        first_requirement_observation_at(values, right, target, candidate),
    ensures left == right,
{
    if left < right {
        assert(!requirement_observation_matches(values[left], target, candidate));
    } else if right < left {
        assert(!requirement_observation_matches(values[right], target, candidate));
    }
}

pub proof fn first_criterion_index_unique(
    values: Seq<CriterionObservation>,
    left: int,
    right: int,
    target: AcceptanceCriterion,
    candidate: IntegratedCandidate,
)
    requires
        first_criterion_observation_at(values, left, target, candidate),
        first_criterion_observation_at(values, right, target, candidate),
    ensures left == right,
{
    if left < right {
        assert(!criterion_observation_matches(values[left], target, candidate));
    } else if right < left {
        assert(!criterion_observation_matches(values[right], target, candidate));
    }
}

pub proof fn requirement_admission_at_first(
    values: Seq<RequirementObservation>,
    index: int,
    target: SecurityRequirement,
    candidate: IntegratedCandidate,
)
    requires first_requirement_observation_at(values, index, target, candidate),
    ensures requirement_admitted(values, target, candidate) == (
        values[index].spec_outcome().spec_is_passed()
            && crate::binding::digest_is_present(values[index].spec_evidence_digest())
    ),
{
    if requirement_admitted(values, target, candidate) {
        let other = choose |other: int| {
            let observation = #[trigger] values[other];
            &&& first_requirement_observation_at(values, other, target, candidate)
            &&& observation.spec_outcome().spec_is_passed()
            &&& crate::binding::digest_is_present(observation.spec_evidence_digest())
        };
        first_requirement_index_unique(values, index, other, target, candidate);
    } else if values[index].spec_outcome().spec_is_passed()
        && crate::binding::digest_is_present(values[index].spec_evidence_digest())
    {
        assert(requirement_admitted(values, target, candidate)) by {
            assert(exists |witness: int| {
                let observation = #[trigger] values[witness];
                &&& witness == index
                &&& first_requirement_observation_at(values, witness, target, candidate)
                &&& observation.spec_outcome().spec_is_passed()
                &&& crate::binding::digest_is_present(observation.spec_evidence_digest())
            });
        }
    }
}

pub proof fn criterion_admission_at_first(
    values: Seq<CriterionObservation>,
    index: int,
    target: AcceptanceCriterion,
    candidate: IntegratedCandidate,
)
    requires first_criterion_observation_at(values, index, target, candidate),
    ensures criterion_admitted(values, target, candidate) == (
        values[index].spec_outcome().spec_is_passed()
            && crate::binding::digest_is_present(values[index].spec_evidence_digest())
    ),
{
    if criterion_admitted(values, target, candidate) {
        let other = choose |other: int| {
            let observation = #[trigger] values[other];
            &&& first_criterion_observation_at(values, other, target, candidate)
            &&& observation.spec_outcome().spec_is_passed()
            &&& crate::binding::digest_is_present(observation.spec_evidence_digest())
        };
        first_criterion_index_unique(values, index, other, target, candidate);
    } else if values[index].spec_outcome().spec_is_passed()
        && crate::binding::digest_is_present(values[index].spec_evidence_digest())
    {
        assert(criterion_admitted(values, target, candidate)) by {
            assert(exists |witness: int| {
                let observation = #[trigger] values[witness];
                &&& witness == index
                &&& first_criterion_observation_at(values, witness, target, candidate)
                &&& observation.spec_outcome().spec_is_passed()
                &&& crate::binding::digest_is_present(observation.spec_evidence_digest())
            });
        }
    }
}

} // verus!
