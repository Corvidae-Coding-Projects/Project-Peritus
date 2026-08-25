//! Reducer-only mutation helpers for otherwise immutable state surfaces.

use peritus_types::{EventId, EventSequence, GateId, Sha256Digest};

use super::{GateRunPhase, GateRunState, GateSlot, GateSlotPhase, GateTerminal};
use crate::{ActiveAttempt, GateAttemptResult, GateEvidenceReceipt, GateTerminalKind};

pub fn prepare(state: &mut GateRunState, slot_index: usize, attempt: ActiveAttempt) {
    state.used_executions.push(attempt.execution_id());
    state.used_actions.push(attempt.action_id());
    let slot = &mut state.slots[slot_index];
    slot.attempts = attempt.ordinal().get();
    slot.active = Some(attempt);
    slot.last_result = None;
    slot.result_event = None;
    slot.evidence = None;
    slot.phase = GateSlotPhase::Prepared;
}

pub const fn dispatch(slot: &mut GateSlot) {
    slot.phase = GateSlotPhase::Dispatched;
}

pub fn observe(
    slot: &mut GateSlot,
    result: GateAttemptResult,
    result_event: EventId,
    phase: GateSlotPhase,
) {
    slot.last_result = Some(result);
    slot.result_event = Some(result_event);
    slot.phase = phase;
}

pub const fn recover(slot: &mut GateSlot, phase: GateSlotPhase) {
    slot.phase = phase;
}

pub fn publish(slot: &mut GateSlot, receipt: GateEvidenceReceipt) {
    slot.evidence = Some(receipt);
    slot.phase = GateSlotPhase::Passed;
}

pub const fn block(slot: &mut GateSlot, dependency: GateId) {
    slot.active = None;
    slot.blocked_by = Some(dependency);
    slot.phase = GateSlotPhase::Blocked;
}

pub const fn cancel(slot: &mut GateSlot) {
    slot.phase = GateSlotPhase::Cancelled;
}

pub const fn advance(
    state: &mut GateRunState,
    sequence: EventSequence,
    event_id: EventId,
    digest: Sha256Digest,
) {
    state.sequence = sequence;
    state.last_event_id = event_id;
    state.state_digest = digest;
}

pub const fn set_state_digest(state: &mut GateRunState, digest: Sha256Digest) {
    state.state_digest = digest;
}

pub const fn set_phase(state: &mut GateRunState, phase: GateRunPhase) {
    state.phase = phase;
}

pub fn terminal(state: &mut GateRunState, terminal: GateTerminal) {
    state.terminal = Some(terminal);
    state.phase = GateRunPhase::Terminal;
}

pub const fn make_terminal(
    kind: GateTerminalKind,
    non_passing: Vec<GateId>,
    digest: Sha256Digest,
) -> GateTerminal {
    GateTerminal { kind, non_passing, digest }
}

pub fn slot_index(state: &GateRunState, gate_id: GateId) -> Option<usize> {
    state.slots.binary_search_by_key(&gate_id, GateSlot::gate_id).ok()
}
