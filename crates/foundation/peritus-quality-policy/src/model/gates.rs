//! Contract-derived gate membership and exact-revision passing results.

#[cfg(verus_only)]
use crate::{AcceptanceEvidence, GateObservation, GateOutcome};
#[cfg(verus_only)]
use peritus_spec::{AcceptanceContract, GateDefinition};
#[cfg(verus_only)]
use peritus_types::{GateId, RevisionTuple};
use vstd::prelude::*;

verus! {

/// Exact byte equality of the declared and observed gate identities.
pub open spec fn gate_ids_match(left: GateId, right: GateId) -> bool {
    crate::revision::same_identifier(left.spec_bytes(), right.spec_bytes())
}

/// The gate identity occurs in the immutable contract declarations.
pub open spec fn gate_declared(definitions: Seq<GateDefinition>, target: GateId) -> bool {
    exists |index: int| 0 <= index < definitions.len()
        && gate_ids_match(#[trigger] definitions[index].spec_id(), target)
}

/// The observation names this gate and the entire requested revision tuple.
pub open spec fn current_gate_matches(
    observation: GateObservation,
    target: GateId,
    requested: RevisionTuple,
) -> bool {
    gate_ids_match(observation.spec_gate_id(), target)
        && crate::model::revision_fresh(observation.spec_revision(), requested)
}

/// This index is the first observation for the gate at the requested revision.
pub open spec fn first_current_gate(
    observations: Seq<GateObservation>,
    target: GateId,
    requested: RevisionTuple,
    index: int,
) -> bool {
    0 <= index < observations.len()
        && current_gate_matches(observations[index], target, requested)
        && (forall |prior: int| 0 <= prior < index ==>
            !current_gate_matches(#[trigger] observations[prior], target, requested))
}

/// The selected current observation exists and passed.
///
/// Canonical evidence has a unique observation per gate. Naming the first matching observation
/// also specifies the evaluator for arbitrary sequences without requiring a caller-side premise.
pub open spec fn first_current_gate_passed(
    observations: Seq<GateObservation>,
    target: GateId,
    requested: RevisionTuple,
) -> bool {
    exists |index: int| #[trigger] first_current_gate(observations, target, requested, index)
        && observations[index].spec_outcome() == GateOutcome::Passed
}

/// Every current gate observation names a gate declared by this contract.
pub open spec fn current_gates_declared(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
) -> bool {
    forall |index: int| 0 <= index < evidence.spec_gates().len()
        && crate::model::revision_fresh(
            #[trigger] evidence.spec_gates()[index].spec_revision(), requested)
        ==> gate_declared(
            contract.spec_gates().spec_definitions(),
            evidence.spec_gates()[index].spec_gate_id(),
        )
}

/// Every contract-declared gate has a passing observation for the exact requested revision.
pub open spec fn required_gates_passed(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
) -> bool {
    forall |index: int| 0 <= index < contract.spec_gates().spec_definitions().len()
        ==> first_current_gate_passed(
            evidence.spec_gates(),
            #[trigger] contract.spec_gates().spec_definitions()[index].spec_id(),
            requested,
        )
}

/// Exact gate-family completeness, independent of evaluator flags and diagnostics.
pub open spec fn required_gates_complete(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    maximum_attempts: u16,
) -> bool {
    current_gates_declared(contract, requested, evidence)
        && required_gates_passed(contract, requested, evidence)
        && crate::model::passing_gate_attempts_within_limit(
            evidence.spec_gates(), requested, maximum_attempts)
}

} // verus!
