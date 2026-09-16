//! Required gate evaluation.

use crate::{AcceptanceEvidence, GateOutcome, UnmetCondition};
use peritus_spec::AcceptanceContract;
use peritus_types::{GateId, RevisionTuple};
use vstd::prelude::*;

verus! {

fn declared(contract: &AcceptanceContract, target: GateId) -> (found: bool)
    ensures found == crate::model::gate_declared(contract.spec_gates().spec_definitions(), target),
{
    let definitions = contract.gates().definitions();
    let mut index = 0;
    while index < definitions.len()
        invariant
            index <= definitions.len(),
            definitions@ == contract.spec_gates().spec_definitions(),
            forall |prior: int| 0 <= prior < index ==>
                !crate::model::gate_ids_match(#[trigger] definitions@[prior].spec_id(), target),
        decreases definitions.len() - index,
    {
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
) -> (result: Option<GateOutcome>)
    ensures
        (result == Some(GateOutcome::Passed)) == crate::model::first_current_gate_passed(
            evidence.spec_gates(), gate_id, requested),
        match result {
            Some(outcome) => exists |index: int|
                #[trigger] crate::model::first_current_gate(
                    evidence.spec_gates(), gate_id, requested, index)
                && evidence.spec_gates()[index].spec_outcome() == outcome,
            None => forall |index: int| 0 <= index < evidence.spec_gates().len() ==>
                !crate::model::current_gate_matches(
                    #[trigger] evidence.spec_gates()[index], gate_id, requested),
        },
{
    let mut index = 0;
    while index < evidence.gates().len()
        invariant
            0 <= index <= evidence.spec_gates().len(),
            forall |prior: int| 0 <= prior < index ==>
                !crate::model::current_gate_matches(
                    #[trigger] evidence.spec_gates()[prior], gate_id, requested),
        decreases evidence.spec_gates().len() - index,
    {
        let observation = &evidence.gates()[index];
        if crate::revision::gate_id_matches(observation.gate_id(), gate_id)
            && crate::revision::revision_matches(observation.revision(), requested)
        {
            assert(crate::model::first_current_gate(
                evidence.spec_gates(), gate_id, requested, index as int));
            assert forall |candidate: int| #[trigger] crate::model::first_current_gate(
                evidence.spec_gates(), gate_id, requested, candidate)
                implies candidate == index as int by {
                if candidate < index {
                    assert(!crate::model::current_gate_matches(
                        evidence.spec_gates()[candidate], gate_id, requested));
                } else if candidate > index {
                    assert(!crate::model::current_gate_matches(
                        evidence.spec_gates()[index as int], gate_id, requested));
                }
            }
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
    ensures
        complete == crate::model::required_gates_complete(
            contract, requested, evidence, maximum_attempts),
        complete ==> crate::model::passing_gate_attempts_within_limit(
            evidence.spec_gates(), requested, maximum_attempts),
        complete ==> final(unmet)@ == old(unmet)@,
{
    let mut complete = true;
    let definitions = contract.gates().definitions();
    let mut observation_index = 0;
    while observation_index < evidence.gates().len()
        invariant
            0 <= observation_index <= evidence.spec_gates().len(),
            definitions@ == contract.spec_gates().spec_definitions(),
            complete == ((forall |prior: int| 0 <= prior < observation_index
                && crate::model::revision_fresh(
                    #[trigger] evidence.spec_gates()[prior].spec_revision(), requested)
                ==> crate::model::gate_declared(
                    contract.spec_gates().spec_definitions(),
                    evidence.spec_gates()[prior].spec_gate_id()))
                && (forall |prior: int| 0 <= prior < observation_index
                    && crate::model::revision_fresh(
                        #[trigger] evidence.spec_gates()[prior].spec_revision(), requested)
                    && evidence.spec_gates()[prior].spec_outcome() == GateOutcome::Passed
                    ==> evidence.spec_gates()[prior].spec_attempt() <= maximum_attempts)),
            complete ==> unmet@ == old(unmet)@,
        decreases evidence.spec_gates().len() - observation_index,
    {
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
    while definition_index < definitions.len()
        invariant
            0 <= definition_index <= definitions.len(),
            definitions@ == contract.spec_gates().spec_definitions(),
            complete == (crate::model::current_gates_declared(contract, requested, evidence)
                && crate::model::passing_gate_attempts_within_limit(
                    evidence.spec_gates(), requested, maximum_attempts)
                && (forall |prior: int| 0 <= prior < definition_index ==>
                    crate::model::first_current_gate_passed(
                        evidence.spec_gates(),
                        #[trigger] definitions@[prior].spec_id(), requested))),
            complete ==> unmet@ == old(unmet)@,
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
