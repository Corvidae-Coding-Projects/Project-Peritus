//! Required gate evaluation.

use crate::{AcceptanceEvidence, GateOutcome, UnmetCondition};
use peritus_spec::AcceptanceContract;
use peritus_types::{GateId, RevisionTuple};
use vstd::prelude::*;

verus! {

fn passing_attempts_within_limit(
    values: &[crate::GateObservation],
    requested: RevisionTuple,
    maximum: u16,
) -> (within_limit: bool)
    ensures within_limit == crate::model::passing_gate_attempts_within_limit(
        values@,
        requested,
        maximum,
    ),
{
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            forall |prior: int| 0 <= prior < index
                && crate::model::revision_fresh(
                    #[trigger] values@[prior].spec_revision(), requested)
                && values@[prior].spec_outcome() == GateOutcome::Passed
                ==> values@[prior].spec_attempt() <= maximum,
        decreases values.len() - index,
    {
        if crate::revision::revision_matches(values[index].revision(), requested) {
            assert(crate::model::revision_fresh(
                values@[index as int].spec_revision(),
                requested,
            ));
            if values[index].passed() {
                assert(values@[index as int].spec_outcome() == GateOutcome::Passed);
                let attempt = values[index].attempt().get();
                if attempt > maximum {
                    assert(values@[index as int].spec_attempt() > maximum);
                    assert(!crate::model::passing_gate_attempts_within_limit(
                        values@,
                        requested,
                        maximum,
                    )) by {
                        assert(crate::model::revision_fresh(
                            values@[index as int].spec_revision(),
                            requested,
                        ));
                    };
                    return false;
                }
                assert(values@[index as int].spec_attempt() <= maximum);
            }
        }
        assert(crate::model::revision_fresh(
            values@[index as int].spec_revision(), requested)
            && values@[index as int].spec_outcome() == GateOutcome::Passed
            ==> values@[index as int].spec_attempt() <= maximum);
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
    while index < evidence.gates().len()
        invariant 0 <= index <= evidence.spec_gates().len(),
        decreases evidence.spec_gates().len() - index,
    {
        let observation = &evidence.gates()[index];
        if observation.gate_id() == gate_id && observation.revision() == requested {
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
) -> (complete: bool)
    ensures complete ==> crate::model::passing_gate_attempts_within_limit(
        evidence.spec_gates(),
        requested,
        maximum_attempts,
    ),
{
    let mut complete = true;
    let definitions = contract.gates().definitions();
    let mut observation_index = 0;
    while observation_index < evidence.gates().len()
        invariant 0 <= observation_index <= evidence.spec_gates().len(),
        decreases evidence.spec_gates().len() - observation_index,
    {
        let observation = &evidence.gates()[observation_index];
        if observation.revision() == requested
            && contract.gates().get(observation.gate_id()).is_none()
        {
            complete = false;
            unmet.push(UnmetCondition::UnknownGate(observation.gate_id()));
        }
        if observation.revision() == requested
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
    while definition_index < definitions.len()
        invariant 0 <= definition_index <= definitions.len(),
        decreases definitions.len() - definition_index,
    {
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
    complete
        && passing_attempts_within_limit(
            evidence.gates(),
            requested,
            maximum_attempts,
        )
}

} // verus!
