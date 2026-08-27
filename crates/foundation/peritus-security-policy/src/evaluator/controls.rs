//! R-SEC and numbered acceptance-criterion completeness.

use crate::{
    AcceptanceCriterion, IntegratedCandidate, SecurityControlOutcome, SecurityEvidence,
    SecurityRequirement, UnmetSecurityCondition,
};
use vstd::prelude::*;

verus! {

fn requirement_observation(
    evidence: &SecurityEvidence,
    target: SecurityRequirement,
    candidate: IntegratedCandidate,
) -> Option<&crate::RequirementObservation> {
    let values = evidence.requirements();
    let mut index = 0;
    while index < values.len()
        invariant 0 <= index <= values.len(),
        decreases values.len() - index,
    {
        if values[index].requirement() == target
            && crate::binding::candidate_matches(values[index].candidate(), candidate)
        {
            return Some(&values[index]);
        }
        index += 1;
    }
    None
}

fn criterion_observation(
    evidence: &SecurityEvidence,
    target: AcceptanceCriterion,
    candidate: IntegratedCandidate,
) -> Option<&crate::CriterionObservation> {
    let values = evidence.criteria();
    let mut index = 0;
    while index < values.len()
        invariant 0 <= index <= values.len(),
        decreases values.len() - index,
    {
        if values[index].criterion() == target
            && crate::binding::candidate_matches(values[index].candidate(), candidate)
        {
            return Some(&values[index]);
        }
        index += 1;
    }
    None
}

pub(super) fn evaluate_requirements(
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
    unmet: &mut Vec<UnmetSecurityCondition>,
) -> bool {
    let mut complete = true;
    let mut index = 0;
    while index < SecurityRequirement::ALL.len()
        invariant 0 <= index <= SecurityRequirement::ALL.len(),
        decreases SecurityRequirement::ALL.len() - index,
    {
        let requirement = SecurityRequirement::ALL[index];
        match requirement_observation(evidence, requirement, candidate) {
            None => {
                complete = false;
                unmet.push(UnmetSecurityCondition::MissingRequirement(requirement));
            }
            Some(observation) => {
                if observation.outcome() != SecurityControlOutcome::Passed {
                    complete = false;
                    unmet.push(UnmetSecurityCondition::RequirementDidNotPass {
                        requirement,
                        outcome: observation.outcome(),
                    });
                }
                if !crate::binding::digest_present(observation.evidence_digest()) {
                    complete = false;
                    unmet.push(UnmetSecurityCondition::EmptyRequirementEvidence(requirement));
                }
            }
        }
        index += 1;
    }
    complete
}

pub(super) fn evaluate_criteria(
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
    unmet: &mut Vec<UnmetSecurityCondition>,
) -> bool {
    let mut complete = true;
    let mut index = 0;
    while index < AcceptanceCriterion::ALL.len()
        invariant 0 <= index <= AcceptanceCriterion::ALL.len(),
        decreases AcceptanceCriterion::ALL.len() - index,
    {
        let criterion = AcceptanceCriterion::ALL[index];
        match criterion_observation(evidence, criterion, candidate) {
            None => {
                complete = false;
                unmet.push(UnmetSecurityCondition::MissingCriterion(criterion));
            }
            Some(observation) => {
                if observation.outcome() != SecurityControlOutcome::Passed {
                    complete = false;
                    unmet.push(UnmetSecurityCondition::CriterionDidNotPass {
                        criterion,
                        outcome: observation.outcome(),
                    });
                }
                if !crate::binding::digest_present(observation.evidence_digest()) {
                    complete = false;
                    unmet.push(UnmetSecurityCondition::EmptyCriterionEvidence(criterion));
                }
            }
        }
        index += 1;
    }
    complete
}

} // verus!
