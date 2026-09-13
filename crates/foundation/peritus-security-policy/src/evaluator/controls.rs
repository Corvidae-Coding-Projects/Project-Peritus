//! R-SEC and numbered acceptance-criterion completeness.

mod model;

use crate::{
    AcceptanceCriterion, IntegratedCandidate, SecurityEvidence, SecurityRequirement,
    UnmetSecurityCondition,
};
use vstd::prelude::*;

verus! {

pub open spec fn requirements_complete(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> bool {
    model::requirements_complete(evidence, candidate)
}

pub open spec fn criteria_complete(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> bool {
    model::criteria_complete(evidence, candidate)
}

const fn requirements_equal(
    left: SecurityRequirement,
    right: SecurityRequirement,
) -> (equal: bool)
    ensures equal == (left == right),
{
    matches!((left, right),
        (SecurityRequirement::RSec001, SecurityRequirement::RSec001)
        | (SecurityRequirement::RSec002, SecurityRequirement::RSec002)
        | (SecurityRequirement::RSec003, SecurityRequirement::RSec003)
        | (SecurityRequirement::RSec004, SecurityRequirement::RSec004)
        | (SecurityRequirement::RSec005, SecurityRequirement::RSec005)
        | (SecurityRequirement::RSec006, SecurityRequirement::RSec006)
        | (SecurityRequirement::RSec007, SecurityRequirement::RSec007)
    )
}

const fn criteria_equal(
    left: AcceptanceCriterion,
    right: AcceptanceCriterion,
) -> (equal: bool)
    ensures equal == (left == right),
{
    matches!((left, right),
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
) -> (result: Option<(usize, &crate::RequirementObservation)>)
    ensures match result {
        Some((index, observation)) => {
            &&& model::first_requirement_observation_at(
                evidence.spec_requirements(), index as int, target, candidate)
            &&& *observation == evidence.spec_requirements()[index as int]
        },
        None => forall |index: int| 0 <= index < evidence.spec_requirements().len() ==>
            !model::requirement_observation_matches(
                #[trigger] evidence.spec_requirements()[index], target, candidate),
    },
{
    let values = evidence.requirements();
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            values@ == evidence.spec_requirements(),
            forall |prior: int| 0 <= prior < index ==>
                !model::requirement_observation_matches(
                    #[trigger] values@[prior], target, candidate),
        decreases values.len() - index,
    {
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
) -> (result: Option<(usize, &crate::CriterionObservation)>)
    ensures match result {
        Some((index, observation)) => {
            &&& model::first_criterion_observation_at(
                evidence.spec_criteria(), index as int, target, candidate)
            &&& *observation == evidence.spec_criteria()[index as int]
        },
        None => forall |index: int| 0 <= index < evidence.spec_criteria().len() ==>
            !model::criterion_observation_matches(
                #[trigger] evidence.spec_criteria()[index], target, candidate),
    },
{
    let values = evidence.criteria();
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            values@ == evidence.spec_criteria(),
            forall |prior: int| 0 <= prior < index ==>
                !model::criterion_observation_matches(
                    #[trigger] values@[prior], target, candidate),
        decreases values.len() - index,
    {
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
) -> (complete: bool)
    ensures
        complete == requirements_complete(evidence, candidate),
        complete ==> final(unmet)@ == old(unmet)@,
{
    let mut complete = true;
    let mut index = 0;
    while index < SecurityRequirement::ALL.len()
        invariant
            0 <= index <= SecurityRequirement::ALL.len(),
            complete == model::requirements_complete_through(
                evidence,
                candidate,
                index as int,
            ),
            complete ==> unmet@ == old(unmet)@,
        decreases SecurityRequirement::ALL.len() - index,
    {
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
                proof {
                    model::requirement_admission_at_first(
                        evidence.spec_requirements(),
                        _observation_index as int,
                        requirement,
                        candidate,
                    );
                };
            }
        }
        proof {
            model::requirements_complete_step(evidence, candidate, index as int);
        }
        index += 1;
    }
    reveal(requirements_complete);
    reveal(model::requirements_complete);
    complete
}

pub(super) fn evaluate_criteria(
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
    unmet: &mut Vec<UnmetSecurityCondition>,
) -> (complete: bool)
    ensures
        complete == criteria_complete(evidence, candidate),
        complete ==> final(unmet)@ == old(unmet)@,
{
    let mut complete = true;
    let mut index = 0;
    while index < AcceptanceCriterion::ALL.len()
        invariant
            0 <= index <= AcceptanceCriterion::ALL.len(),
            complete == model::criteria_complete_through(evidence, candidate, index as int),
            complete ==> unmet@ == old(unmet)@,
        decreases AcceptanceCriterion::ALL.len() - index,
    {
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
                    unmet.push(UnmetSecurityCondition::CriterionDidNotPass {
                        criterion,
                        outcome,
                    });
                }
                if !crate::binding::digest_present(observation.evidence_digest()) {
                    complete = false;
                    unmet.push(UnmetSecurityCondition::EmptyCriterionEvidence(criterion));
                }
                proof {
                    model::criterion_admission_at_first(
                        evidence.spec_criteria(),
                        _observation_index as int,
                        criterion,
                        candidate,
                    );
                };
            }
        }
        proof {
            model::criteria_complete_step(evidence, candidate, index as int);
        }
        index += 1;
    }
    reveal(criteria_complete);
    reveal(model::criteria_complete);
    complete
}

} // verus!
