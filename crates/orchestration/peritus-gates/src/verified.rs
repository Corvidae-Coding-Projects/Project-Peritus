//! Executable predicates and Verus proof roots for D1 safety invariants.

use peritus_types::{GateId, RevisionTuple};
use vstd::prelude::*;

use crate::{GatePlan, GateRunState, GateSlotPhase, GateTerminalKind};

verus! {

/// Mathematical bounded-attempt accounting predicate.
pub open spec fn bounded_attempts(attempts: int, maximum: int) -> bool {
    0 <= attempts && attempts <= maximum
}

/// Mathematical terminal truth predicate: passing requires every gate to pass.
pub open spec fn passing_terminal(all_gates_passed: bool, claimed_pass: bool) -> bool {
    !claimed_pass || all_gates_passed
}

/// Mathematical no-implicit-success predicate.
pub open spec fn explicit_success(
    result_complete: bool,
    evidence_complete: bool,
    claimed_pass: bool,
) -> bool {
    !claimed_pass || (result_complete && evidence_complete)
}

/// Mathematical legality of a dependency's position in a topological schedule.
pub open spec fn ordered_dependency(dependency_index: int, gate_index: int) -> bool {
    0 <= dependency_index && dependency_index < gate_index
}

/// Mathematical exact-replay claim over canonical digest projections.
pub open spec fn exact_replay(expected_digest: int, observed_digest: int, claimed: bool) -> bool {
    !claimed || expected_digest == observed_digest
}

/// Mathematical freshness claim over equality of every revision component.
pub open spec fn exact_freshness(all_components_equal: bool, claimed: bool) -> bool {
    !claimed || all_components_equal
}

/// Proves that a checked attempt increment below a cap remains bounded.
pub proof fn bounded_successor(attempts: int, maximum: int)
    requires 0 <= attempts, attempts < maximum
    ensures bounded_attempts(attempts + 1, maximum)
{
}

/// Proves that no passing claim follows from absent result or evidence.
pub proof fn missing_input_cannot_pass(result_complete: bool, evidence_complete: bool)
    requires !result_complete || !evidence_complete
    ensures !explicit_success(result_complete, evidence_complete, true)
{
}

/// Proves a dependency at any nonnegative earlier index is legally ordered.
pub proof fn earlier_dependency_is_legal(dependency_index: int, gate_index: int)
    requires 0 <= dependency_index, dependency_index < gate_index
    ensures ordered_dependency(dependency_index, gate_index)
{
}

/// Proves reflexive canonical replay equivalence.
pub proof fn replay_reflexive(digest: int)
    ensures exact_replay(digest, digest, true)
{
}

/// Proves a freshness claim cannot hold when a revision component differs.
pub proof fn stale_revision_cannot_be_fresh()
    ensures !exact_freshness(false, true)
{
}

/// Proves that an all-passing aggregate may truthfully claim pass.
pub proof fn all_passed_allows_terminal()
    ensures passing_terminal(true, true)
{
}

} // verus!

/// Returns whether every gate attempt count is within the immutable plan cap.
#[must_use]
pub fn attempts_are_bounded(plan: &GatePlan, state: &GateRunState) -> bool {
    state.maximum_attempts() == plan.maximum_attempts()
        && state.slots().iter().all(|slot| slot.attempts() <= plan.maximum_attempts())
        && state.used_executions().len()
            == state.slots().iter().map(|slot| usize::from(slot.attempts())).sum::<usize>()
        && state.used_actions().len() == state.used_executions().len()
}

/// Returns whether all declared prerequisites currently have fresh passing evidence.
#[must_use]
pub fn dependencies_are_satisfied(plan: &GatePlan, state: &GateRunState, gate_id: GateId) -> bool {
    plan.gate(gate_id).is_some_and(|gate| {
        gate.dependencies().iter().all(|dependency| {
            state.slot(*dependency).is_some_and(|slot| {
                slot.phase() == GateSlotPhase::Passed
                    && slot.evidence().is_some_and(|receipt| receipt.revision() == state.revision())
            })
        })
    })
}

/// Returns whether every planned dependency precedes its consumer in the immutable schedule.
#[must_use]
pub fn dependency_order_is_legal(plan: &GatePlan) -> bool {
    plan.execution_order().iter().enumerate().all(|(gate_index, gate_id)| {
        plan.gate(*gate_id).is_some_and(|gate| {
            gate.dependencies().iter().all(|dependency| {
                plan.execution_order()[..gate_index].iter().any(|candidate| candidate == dependency)
            })
        })
    })
}

/// Returns exact equality of all revision components on an evidence receipt.
#[must_use]
pub fn evidence_is_fresh(observed: RevisionTuple, requested: RevisionTuple) -> bool {
    observed == requested
}

/// Returns whether the terminal claim agrees with canonical per-gate state.
#[must_use]
pub fn terminal_truthful(state: &GateRunState) -> bool {
    state.terminal().map_or_else(
        || state.phase() != crate::GateRunPhase::Terminal,
        |terminal| match terminal.kind() {
            GateTerminalKind::Passed => state.slots().iter().all(|slot| {
                slot.phase() == GateSlotPhase::Passed
                    && slot.last_result().is_some_and(crate::GateAttemptResult::passed)
                    && slot.evidence().is_some_and(|receipt| receipt.revision() == state.revision())
            }),
            GateTerminalKind::Failed
            | GateTerminalKind::Cancelled
            | GateTerminalKind::Indeterminate => !terminal.non_passing().is_empty(),
        },
    )
}

/// Returns the executable no-implicit-success invariant.
#[must_use]
pub fn no_implicit_success(state: &GateRunState) -> bool {
    state.slots().iter().all(|slot| {
        slot.phase() != GateSlotPhase::Passed
            || (slot.last_result().is_some_and(crate::GateAttemptResult::passed)
                && slot.evidence().is_some_and(|receipt| receipt.revision() == state.revision()))
    })
}

/// Returns exact replay equivalence over complete state and canonical state bytes.
#[must_use]
pub fn replay_equivalent(expected: &GateRunState, observed: &GateRunState) -> bool {
    expected == observed
        && crate::canonical::state_bytes(expected) == crate::canonical::state_bytes(observed)
}
