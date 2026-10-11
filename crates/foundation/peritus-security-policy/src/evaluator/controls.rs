//! R-SEC and numbered acceptance-criterion completeness.

mod model;

use crate::{
    AcceptanceCriterion, IntegratedCandidate, SecurityEvidence, SecurityRequirement,
    UnmetSecurityCondition,
};

const fn requirements_equal(left: SecurityRequirement, right: SecurityRequirement) -> bool {
    matches!(
        (left, right),
        (SecurityRequirement::RSec001, SecurityRequirement::RSec001)
            | (SecurityRequirement::RSec002, SecurityRequirement::RSec002)
            | (SecurityRequirement::RSec003, SecurityRequirement::RSec003)
            | (SecurityRequirement::RSec004, SecurityRequirement::RSec004)
            | (SecurityRequirement::RSec005, SecurityRequirement::RSec005)
            | (SecurityRequirement::RSec006, SecurityRequirement::RSec006)
            | (SecurityRequirement::RSec007, SecurityRequirement::RSec007)
    )
}
const fn criteria_equal(left: AcceptanceCriterion, right: AcceptanceCriterion) -> bool {
    matches!(
        (left, right),
        (AcceptanceCriterion::Criterion9, AcceptanceCriterion::Criterion9)
            | (AcceptanceCriterion::Criterion10, AcceptanceCriterion::Criterion10)
            | (AcceptanceCriterion::Criterion11, AcceptanceCriterion::Criterion11)
            | (AcceptanceCriterion::Criterion12, AcceptanceCriterion::Criterion12)
            | (AcceptanceCriterion::Criterion17, AcceptanceCriterion::Criterion17)
            | (AcceptanceCriterion::Criterion18, AcceptanceCriterion::Criterion18)
            | (AcceptanceCriterion::Criterion19, AcceptanceCriterion::Criterion19)
            | (AcceptanceCriterion::Criterion24, AcceptanceCriterion::Criterion24)
            | (AcceptanceCriterion::Criterion25, AcceptanceCriterion::Criterion25)
    )
}
fn requirement_observation(
    evidence: &SecurityEvidence,
    target: SecurityRequirement,
    candidate: IntegratedCandidate,
) -> Option<(usize, &crate::RequirementObservation)> {
    let values = evidence.requirements();
    let mut index = 0;
    while index < values.len() {
        if requirements_equal(values[index].requirement(), target)
            && crate::binding::candidate_matches(values[index].candidate(), candidate)
        {
            return Some((index, &values[index]));
        }
        index += 1;
    }
    None
}
fn criterion_observation(
    evidence: &SecurityEvidence,
    target: AcceptanceCriterion,
    candidate: IntegratedCandidate,
) -> Option<(usize, &crate::CriterionObservation)> {
    let values = evidence.criteria();
    let mut index = 0;
    while index < values.len() {
        if criteria_equal(values[index].criterion(), target)
            && crate::binding::candidate_matches(values[index].candidate(), candidate)
        {
            return Some((index, &values[index]));
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
    while index < SecurityRequirement::ALL.len() {
        let requirement = SecurityRequirement::ALL[index];
        match requirement_observation(evidence, requirement, candidate) {
            None => {
                complete = false;
                unmet.push(UnmetSecurityCondition::MissingRequirement(requirement));
            }
            Some((_observation_index, observation)) => {
                let outcome = observation.outcome();
                if !outcome.is_passed() {
                    complete = false;
                    unmet.push(UnmetSecurityCondition::RequirementDidNotPass {
                        requirement,
                        outcome,
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
    while index < AcceptanceCriterion::ALL.len() {
        let criterion = AcceptanceCriterion::ALL[index];
        match criterion_observation(evidence, criterion, candidate) {
            None => {
                complete = false;
                unmet.push(UnmetSecurityCondition::MissingCriterion(criterion));
            }
            Some((_observation_index, observation)) => {
                let outcome = observation.outcome();
                if !outcome.is_passed() {
                    complete = false;
                    unmet.push(UnmetSecurityCondition::CriterionDidNotPass { criterion, outcome });
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
