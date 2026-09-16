//! Required artifact completeness checks.

use crate::{AcceptanceEvidence, UnmetCondition};
use peritus_spec::{AcceptanceContract, EvidenceRequirementId};
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

fn declared(contract: &AcceptanceContract, target: EvidenceRequirementId) -> (found: bool)
    ensures found == crate::model::artifact_declared(contract.spec_evidence_requirements(), target),
{
    let requirements = contract.evidence_requirements();
    let mut index = 0;
    while index < requirements.len()
        invariant
            0 <= index <= requirements.len(),
            requirements@ == contract.spec_evidence_requirements(),
            forall |prior: int| 0 <= prior < index ==>
                !crate::model::evidence_requirement_matches(
                    #[trigger] requirements@[prior].spec_id(), target),
        decreases requirements.len() - index,
    {
        if crate::revision::evidence_requirement_matches(requirements[index].id(), target) {
            return true;
        }
        index += 1;
    }
    false
}

fn current(
    evidence: &AcceptanceEvidence,
    target: EvidenceRequirementId,
    requested: RevisionTuple,
) -> (found: bool)
    ensures found == crate::model::current_artifact_present(
        evidence.spec_evidence(), target, requested),
{
    let mut index = 0;
    while index < evidence.evidence().len()
        invariant
            0 <= index <= evidence.spec_evidence().len(),
            forall |prior: int| 0 <= prior < index ==>
                !(crate::model::evidence_requirement_matches(
                    #[trigger] evidence.spec_evidence()[prior].spec_requirement_id(), target)
                    && crate::model::revision_fresh(
                        evidence.spec_evidence()[prior].spec_revision(), requested)),
        decreases evidence.spec_evidence().len() - index,
    {
        if crate::revision::evidence_requirement_matches(
            evidence.evidence()[index].requirement_id(), target)
            && crate::revision::revision_matches(evidence.evidence()[index].revision(), requested)
        {
            return true;
        }
        index += 1;
    }
    false
}

pub(super) fn evaluate(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    unmet: &mut Vec<UnmetCondition>,
) -> (complete: bool)
    ensures
        complete == crate::model::required_artifacts_complete(contract, requested, evidence),
        complete ==> final(unmet)@ == old(unmet)@,
{
    let mut complete = true;
    let mut observation_index = 0;
    while observation_index < evidence.evidence().len()
        invariant
            0 <= observation_index <= evidence.spec_evidence().len(),
            complete == (forall |prior: int| 0 <= prior < observation_index
                && crate::model::revision_fresh(
                    #[trigger] evidence.spec_evidence()[prior].spec_revision(), requested)
                ==> crate::model::artifact_declared(
                    contract.spec_evidence_requirements(),
                    evidence.spec_evidence()[prior].spec_requirement_id())),
            complete ==> unmet@ == old(unmet)@,
        decreases evidence.spec_evidence().len() - observation_index,
    {
        let observation = &evidence.evidence()[observation_index];
        if crate::revision::revision_matches(observation.revision(), requested)
            && !declared(contract, observation.requirement_id())
        {
            complete = false;
            unmet.push(UnmetCondition::UnknownEvidence(observation.requirement_id()));
        }
        observation_index += 1;
    }

    let requirements = contract.evidence_requirements();
    let mut requirement_index = 0;
    while requirement_index < requirements.len()
        invariant
            0 <= requirement_index <= requirements.len(),
            requirements@ == contract.spec_evidence_requirements(),
            complete == (crate::model::current_artifacts_declared(contract, requested, evidence)
                && (forall |prior: int| 0 <= prior < requirement_index ==>
                    crate::model::current_artifact_present(
                        evidence.spec_evidence(),
                        #[trigger] requirements@[prior].spec_id(), requested))),
            complete ==> unmet@ == old(unmet)@,
        decreases requirements.len() - requirement_index,
    {
        let requirement_id = requirements[requirement_index].id();
        if !current(evidence, requirement_id, requested) {
            complete = false;
            unmet.push(UnmetCondition::MissingEvidence(requirement_id));
        }
        requirement_index += 1;
    }
    complete
}

pub(super) fn has_current(
    evidence: &AcceptanceEvidence,
    target: EvidenceRequirementId,
    requested: RevisionTuple,
) -> (found: bool)
    ensures found == crate::model::current_artifact_present(
        evidence.spec_evidence(), target, requested),
{
    current(evidence, target, requested)
}

} // verus!
