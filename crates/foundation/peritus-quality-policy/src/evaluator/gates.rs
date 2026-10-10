//! Required gate evaluation.

use crate::{AcceptanceEvidence, GateOutcome, UnmetCondition};
use peritus_spec::AcceptanceContract;
use peritus_types::{GateId, RevisionTuple};

fn declared(contract: &AcceptanceContract, target: GateId) -> bool {
    let definitions = contract.gates().definitions();
    let mut index = 0;
    while index < definitions.len() {
        if crate::revision::gate_id_matches(definitions[index].id(), target) {
            return true;
        }
        index += 1;
    }
    false
}
fn passing_attempts_within_limit(
    values: &[crate::GateObservation],
    requested: RevisionTuple,
    maximum: u16,
) -> bool {
    let mut index = 0;
    while index < values.len() {
        if crate::revision::revision_matches(values[index].revision(), requested) {
            if values[index].passed() {
                let attempt = values[index].attempt().get();
                if attempt > maximum {
                    return false;
                }
            }
        }
        index += 1;
    }
    true
}
fn current_gate(
    evidence: &AcceptanceEvidence,
    gate_id: GateId,
    requested: RevisionTuple,
) -> Option<GateOutcome> {
    let mut index = 0;
    while index < evidence.gates().len() {
        let observation = &evidence.gates()[index];
        if crate::revision::gate_id_matches(observation.gate_id(), gate_id)
            && crate::revision::revision_matches(observation.revision(), requested)
        {
            return Some(observation.outcome());
        }
        index += 1;
    }
    None
}
pub(super) fn evaluate(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    maximum_attempts: u16,
    unmet: &mut Vec<UnmetCondition>,
) -> bool {
    let mut complete = true;
    let definitions = contract.gates().definitions();
    let mut observation_index = 0;
    while observation_index < evidence.gates().len() {
        let observation = &evidence.gates()[observation_index];
        if crate::revision::revision_matches(observation.revision(), requested)
            && !declared(contract, observation.gate_id())
        {
            complete = false;
            unmet.push(UnmetCondition::UnknownGate(observation.gate_id()));
        }
        if crate::revision::revision_matches(observation.revision(), requested)
            && observation.passed()
            && observation.attempt().get() > maximum_attempts
        {
            complete = false;
            unmet.push(UnmetCondition::GateAttemptLimitExceeded {
                gate_id: observation.gate_id(),
                attempt: observation.attempt().get(),
                maximum: maximum_attempts,
            });
        }
        observation_index += 1;
    }
    let mut definition_index = 0;
    while definition_index < definitions.len() {
        let gate_id = definitions[definition_index].id();
        match current_gate(evidence, gate_id, requested) {
            None => {
                complete = false;
                unmet.push(UnmetCondition::MissingGate(gate_id));
            }
            Some(GateOutcome::Passed) => {}
            Some(GateOutcome::Failed(failure)) => {
                complete = false;
                unmet.push(UnmetCondition::GateDidNotPass { gate_id, failure });
            }
        }
        definition_index += 1;
    }
    complete && passing_attempts_within_limit(evidence.gates(), requested, maximum_attempts)
}
