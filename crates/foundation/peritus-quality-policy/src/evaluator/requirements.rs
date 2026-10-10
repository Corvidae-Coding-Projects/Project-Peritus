//! Required artifact completeness checks.

use crate::{AcceptanceEvidence, UnmetCondition};
use peritus_spec::{AcceptanceContract, EvidenceRequirementId};
use peritus_types::RevisionTuple;

fn declared(contract: &AcceptanceContract, target: EvidenceRequirementId) -> bool {
    let requirements = contract.evidence_requirements();
    let mut index = 0;
    while index < requirements.len() {
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
) -> bool {
    let mut index = 0;
    while index < evidence.evidence().len() {
        if crate::revision::evidence_requirement_matches(
            evidence.evidence()[index].requirement_id(),
            target,
        ) && crate::revision::revision_matches(evidence.evidence()[index].revision(), requested)
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
) -> bool {
    let mut complete = true;
    let mut observation_index = 0;
    while observation_index < evidence.evidence().len() {
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
    while requirement_index < requirements.len() {
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
) -> bool {
    current(evidence, target, requested)
}
