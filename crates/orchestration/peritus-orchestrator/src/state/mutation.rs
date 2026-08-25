//! Reducer-confined E0 state mutation primitives.

use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};

use super::OrchestratorState;
use crate::{
    AcceptanceCertificate, CandidateBinding, ChildAggregateKind, ChildObservation, Handoff,
    HandoffActivationObservation, OrchestratorError, OrchestratorErrorKind, OrchestratorPhase,
    OrchestratorRecoveryAction, OrchestratorTerminal, PendingDirective, QualityCycleBinding,
    ResumeReconciliation,
};

/// Independently bounded counter selected by a reducer transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CounterKind {
    /// Completed candidate revisions.
    Revisions,
    /// Activated writer cycles.
    WriterCycles,
    /// Activated fixer cycles.
    FixerCycles,
    /// Completed gate cycles.
    GateCycles,
    /// Completed review cycles.
    ReviewCycles,
    /// Committed role handoffs.
    Handoffs,
    /// Published child directives.
    ChildDirectives,
    /// Retained authoritative child observations.
    RetainedObservations,
    /// Completed cancellation reconciliation steps.
    CancellationReconciliations,
}

/// Replaces the aggregate lifecycle phase.
pub const fn set_phase(state: &mut OrchestratorState, phase: OrchestratorPhase) {
    state.phase = phase;
}

/// Replaces the unique pending directive.
pub const fn set_pending_directive(state: &mut OrchestratorState, value: Option<PendingDirective>) {
    state.pending_directive = value;
}

/// Borrows the pending directive for reducer-confined delivery mutation.
pub const fn pending_directive_mut(state: &mut OrchestratorState) -> Option<&mut PendingDirective> {
    state.pending_directive.as_mut()
}

/// Replaces the currently open role handoff.
pub fn set_open_handoff(state: &mut OrchestratorState, value: Option<Handoff>) {
    state.open_handoff = value;
}

/// Appends an immutable handoff to causal history.
pub fn push_handoff(state: &mut OrchestratorState, value: Handoff) {
    state.handoffs.push(value);
}

/// Appends an authoritative D3 activation observation.
pub fn push_activation(state: &mut OrchestratorState, value: HandoffActivationObservation) {
    state.activations.push(value);
}

/// Appends an authoritative child observation.
pub fn push_observation(state: &mut OrchestratorState, value: ChildObservation) {
    state.observations.push(value);
}

/// Inserts a child kind into the canonical active set.
pub fn insert_active_child(state: &mut OrchestratorState, kind: ChildAggregateKind) {
    if let Err(index) = state.active_children.binary_search(&kind) {
        state.active_children.insert(index, kind);
    }
}

/// Removes a child kind from the canonical active set and reports whether it existed.
pub fn remove_active_child(state: &mut OrchestratorState, kind: ChildAggregateKind) -> bool {
    state.active_children.binary_search(&kind).is_ok_and(|index| {
        state.active_children.remove(index);
        true
    })
}

/// Replaces the candidate awaiting revision advancement.
pub fn set_proposed_candidate(state: &mut OrchestratorState, value: Option<CandidateBinding>) {
    state.proposed_candidate = value;
}

/// Atomically installs writer output and its same-revision child-cycle binding.
pub fn install_writer_candidate(
    state: &mut OrchestratorState,
    candidate: CandidateBinding,
    quality_cycle: QualityCycleBinding,
) {
    state.current_candidate = candidate.clone();
    if let Some(current) = state.candidate_history.last_mut() {
        *current = candidate;
    }
    state.current_quality_cycle = quality_cycle.clone();
    if let Some(current) = state.quality_cycle_history.last_mut() {
        *current = quality_cycle;
    }
}

/// Advances the candidate and child-cycle histories together.
pub fn advance_candidate(
    state: &mut OrchestratorState,
    value: CandidateBinding,
    quality_cycle: QualityCycleBinding,
) {
    state.current_candidate = value.clone();
    state.candidate_history.push(value);
    state.current_quality_cycle = quality_cycle.clone();
    state.quality_cycle_history.push(quality_cycle);
    state.proposed_candidate = None;
    state.acceptance_certificate = None;
}

/// Replaces the paused child-head reconciliation checkpoint.
pub fn set_paused_reconciliation(
    state: &mut OrchestratorState,
    value: Option<ResumeReconciliation>,
) {
    state.paused_reconciliation = value;
}

/// Clears all recorded child pause acknowledgements.
pub fn clear_paused_children(state: &mut OrchestratorState) {
    state.paused_children.clear();
}

/// Inserts a child into the canonical paused set.
pub fn insert_paused_child(state: &mut OrchestratorState, kind: ChildAggregateKind) {
    if let Err(index) = state.paused_children.binary_search(&kind) {
        state.paused_children.insert(index, kind);
    }
}

/// Replaces the retained acceptance certificate.
pub const fn set_certificate(state: &mut OrchestratorState, value: Option<AcceptanceCertificate>) {
    state.acceptance_certificate = value;
}

/// Records the committed cancellation cause digest.
pub const fn set_cancellation_cause(state: &mut OrchestratorState, value: Sha256Digest) {
    state.cancellation_cause = Some(value);
}

/// Commits a quiescent terminal fact and clears transient ownership.
pub fn set_terminal(state: &mut OrchestratorState, terminal: OrchestratorTerminal) {
    if terminal.kind() != crate::OrchestratorTerminalKind::Cancelled {
        state.cancellation_cause = None;
    }
    state.terminal = Some(terminal);
    state.pending_terminal = None;
    state.open_handoff = None;
    state.proposed_candidate = None;
    state.pending_directive = None;
    state.paused_reconciliation = None;
    state.paused_children.clear();
    state.phase = OrchestratorPhase::Terminal;
}

/// Retains terminal truth while owned children are reconciled.
pub const fn set_pending_terminal(state: &mut OrchestratorState, terminal: OrchestratorTerminal) {
    state.pending_terminal = Some(terminal);
    state.phase = OrchestratorPhase::Cancelling;
}

/// Increments one independently bounded aggregate counter.
///
/// # Errors
/// Returns an error if the counter overflows or exceeds its configured limit.
pub fn increment_counter(
    state: &mut OrchestratorState,
    kind: CounterKind,
) -> Result<(), OrchestratorError> {
    let mut counters = state.counters;
    let selected = match kind {
        CounterKind::Revisions => &mut counters.revisions,
        CounterKind::WriterCycles => &mut counters.writer_cycles,
        CounterKind::FixerCycles => &mut counters.fixer_cycles,
        CounterKind::GateCycles => &mut counters.gate_cycles,
        CounterKind::ReviewCycles => &mut counters.review_cycles,
        CounterKind::Handoffs => &mut counters.handoffs,
        CounterKind::ChildDirectives => &mut counters.child_directives,
        CounterKind::RetainedObservations => &mut counters.retained_observations,
        CounterKind::CancellationReconciliations => &mut counters.cancellation_reconciliations,
    };
    *selected = selected.checked_add(1).ok_or_else(counter_overflow)?;
    counters.validate(&state.binding)?;
    state.counters = counters;
    Ok(())
}

/// Advances the event cursor and records the consumed command identity.
pub fn advance_cursor(
    state: &mut OrchestratorState,
    sequence: EventSequence,
    event_id: EventId,
    command_id: CommandId,
) {
    state.sequence = sequence;
    state.last_event_id = event_id;
    state.used_commands.push(command_id);
}

/// Replaces the canonical complete-state digest.
pub const fn set_state_digest(state: &mut OrchestratorState, digest: Sha256Digest) {
    state.state_digest = digest;
}

const fn counter_overflow() -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::LimitExceeded,
        OrchestratorRecoveryAction::NeedsHuman,
        "orchestrator counter overflowed",
    )
}
