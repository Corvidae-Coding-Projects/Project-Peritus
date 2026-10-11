//! Executable predicates and Verus proof roots for D1 safety invariants.

use peritus_types::{GateId, RevisionTuple};

use crate::{GatePlan, GateRunState, GateSlotPhase, GateTerminalKind};

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
