//! Pure deterministic D1 transition, aggregation, and replay.

mod apply;

use peritus_types::{EventSequence, GateExecutionId, GateId, Sha256Digest};

use crate::error::{GateError, GateRejection, reject};
use crate::state::mutation;
use crate::{
    GateCommand, GateCommandKind, GateEvent, GateEventKind, GatePlan, GateRunPhase, GateRunState,
    GateSlotPhase, GateTerminalKind, GateTransition, RetryPermission,
};

use apply::apply;

/// Starts a run from the only legal genesis command.
///
/// # Errors
/// Rejects any non-genesis fence, plan/revision/run mismatch, or non-start command.
pub fn start(plan: &GatePlan, command: &GateCommand) -> Result<GateTransition, GateError> {
    let GateCommandKind::StartRun { snapshot_digest } = command.kind() else {
        return Err(illegal("genesis command is not StartRun"));
    };
    if command.run_id() != plan.run_id()
        || command.revision() != plan.revision()
        || command.expected_sequence() != 0
        || command.expected_previous_event().is_some()
        || command.prior_state_digest() != Sha256Digest::new([0; 32])
    {
        return Err(reject(
            GateRejection::BindingMismatch,
            "genesis command differs from the exact plan or genesis fences",
        ));
    }
    let sequence = EventSequence::first();
    let mut state = GateRunState::genesis(plan, *snapshot_digest, sequence, command.event_id());
    let successor = crate::canonical::state_digest(&state);
    mutation::set_state_digest(&mut state, successor);
    let event = GateEvent::new(
        command.event_id(),
        command.command_id(),
        sequence,
        None,
        command.run_id(),
        command.revision(),
        Sha256Digest::new([0; 32]),
        successor,
        GateEventKind::RunStarted { snapshot_digest: *snapshot_digest },
    );
    Ok(GateTransition::new(event, state))
}

/// Applies one fenced command to current state without performing effects.
///
/// # Errors
/// Rejects stale fences, illegal phases, unsatisfied dependencies, reused identities, attempt
/// overrun, premature finalization, or evidence/recovery mismatches.
pub fn decide(
    plan: &GatePlan,
    state: &GateRunState,
    command: &GateCommand,
) -> Result<GateTransition, GateError> {
    validate_fences(plan, state, command)?;
    let sequence = state
        .sequence()
        .checked_next()
        .map_err(|_| reject(GateRejection::LimitExceeded, "gate event sequence overflowed"))?;
    let mut successor = state.clone();
    let kind = apply(plan, &mut successor, command.event_id(), command.kind())?;
    propagate_blocks(plan, &mut successor);
    mutation::advance(&mut successor, sequence, command.event_id(), Sha256Digest::new([0; 32]));
    let successor_digest = crate::canonical::state_digest(&successor);
    mutation::set_state_digest(&mut successor, successor_digest);
    let event = GateEvent::new(
        command.event_id(),
        command.command_id(),
        sequence,
        Some(state.last_event_id()),
        command.run_id(),
        command.revision(),
        state.state_digest(),
        successor_digest,
        kind,
    );
    Ok(GateTransition::new(event, successor))
}

pub fn decide_lifecycle(
    state: &GateRunState,
    command: &GateCommand,
) -> Result<GateTransition, GateError> {
    validate_lifecycle_fences(state, command)?;
    let sequence = state
        .sequence()
        .checked_next()
        .map_err(|_| reject(GateRejection::LimitExceeded, "gate event sequence overflowed"))?;
    let mut successor = state.clone();
    let kind = apply::apply_lifecycle(&mut successor, command.kind())?;
    mutation::advance(&mut successor, sequence, command.event_id(), Sha256Digest::new([0; 32]));
    let successor_digest = crate::canonical::state_digest(&successor);
    mutation::set_state_digest(&mut successor, successor_digest);
    let event = GateEvent::new(
        command.event_id(),
        command.command_id(),
        sequence,
        Some(state.last_event_id()),
        command.run_id(),
        command.revision(),
        state.state_digest(),
        successor_digest,
        kind,
    );
    Ok(GateTransition::new(event, successor))
}

/// Reconstructs exactly the same state from genesis and canonical events.
///
/// # Errors
/// Rejects empty, duplicated, reordered, stale, tampered, or semantically illegal event streams.
pub fn replay(plan: &GatePlan, events: &[GateEvent]) -> Result<GateRunState, GateError> {
    let first = events
        .first()
        .ok_or_else(|| reject(GateRejection::ReplayMismatch, "gate replay is empty"))?;
    let first_command = command_from_event(first, 0, None)?;
    let first_transition = start(plan, &first_command)?;
    if first_transition.event() != first {
        return Err(replay_error("genesis gate event differs from deterministic reduction"));
    }
    let mut state = first_transition.into_state();
    let mut identities = std::collections::BTreeSet::from([first.id()]);
    for event in &events[1..] {
        if !identities.insert(event.id()) {
            return Err(replay_error("gate event identity is duplicated"));
        }
        let command =
            command_from_event(event, state.sequence().get(), Some(state.last_event_id()))?;
        let transition = decide(plan, &state, &command)?;
        if transition.event() != event {
            return Err(replay_error("gate event differs from deterministic reduction"));
        }
        state = transition.into_state();
    }
    Ok(state)
}

fn validate_fences(
    plan: &GatePlan,
    state: &GateRunState,
    command: &GateCommand,
) -> Result<(), GateError> {
    let mismatches = [
        state.phase() == GateRunPhase::Terminal,
        state.run_id() != plan.run_id(),
        state.plan_digest() != plan.digest(),
        state.revision() != plan.revision(),
        command.run_id() != state.run_id(),
        command.revision() != state.revision(),
        command.expected_sequence() != state.sequence().get(),
        command.expected_previous_event() != Some(state.last_event_id()),
        command.prior_state_digest() != state.state_digest(),
        matches!(command.kind(), GateCommandKind::StartRun { .. }),
    ];
    if mismatches.into_iter().any(core::convert::identity) {
        return Err(reject(
            GateRejection::ReplayMismatch,
            "gate command predecessor, plan, revision, or lifecycle fence differs",
        ));
    }
    Ok(())
}

fn validate_lifecycle_fences(state: &GateRunState, command: &GateCommand) -> Result<(), GateError> {
    let lifecycle =
        matches!(command.kind(), GateCommandKind::PauseRun | GateCommandKind::ResumeRun);
    let mismatches = [
        state.phase() == GateRunPhase::Terminal,
        command.run_id() != state.run_id(),
        command.revision() != state.revision(),
        command.expected_sequence() != state.sequence().get(),
        command.expected_previous_event() != Some(state.last_event_id()),
        command.prior_state_digest() != state.state_digest(),
        !lifecycle,
    ];
    if mismatches.into_iter().any(core::convert::identity) {
        return Err(reject(
            GateRejection::ReplayMismatch,
            "gate lifecycle command or predecessor fence differs from the durable checkpoint",
        ));
    }
    Ok(())
}

fn propagate_blocks(plan: &GatePlan, state: &mut GateRunState) {
    loop {
        let mut changed = false;
        for gate_id in plan.execution_order() {
            let Some(slot) = state.slot(*gate_id) else { continue };
            if slot.phase() != GateSlotPhase::Pending {
                continue;
            }
            let Some(blocker) = plan.gate(*gate_id).and_then(|gate| {
                gate.dependencies().iter().copied().find(|dependency| {
                    state.slot(*dependency).is_some_and(|dependency| {
                        matches!(
                            dependency.phase(),
                            GateSlotPhase::Failed
                                | GateSlotPhase::Blocked
                                | GateSlotPhase::Cancelled
                        )
                    })
                })
            }) else {
                continue;
            };
            if let Some(slot) = state.slot_mut(*gate_id) {
                mutation::block(slot, blocker);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

pub fn finalize(state: &mut GateRunState) -> Result<(), GateError> {
    if matches!(state.phase(), GateRunPhase::Paused(_)) {
        return Err(illegal("paused gate run must resume or cancel before finalization"));
    }
    if state.slots().iter().any(|slot| {
        !matches!(
            slot.phase(),
            GateSlotPhase::Passed
                | GateSlotPhase::Failed
                | GateSlotPhase::Blocked
                | GateSlotPhase::Cancelled
        )
    }) {
        return Err(reject(
            GateRejection::CancellationIncomplete,
            "gate run still has runnable, active, recovery, or evidence-pending work",
        ));
    }
    let non_passing = state
        .slots()
        .iter()
        .filter(|slot| slot.phase() != GateSlotPhase::Passed)
        .map(crate::GateSlot::gate_id)
        .collect::<Vec<_>>();
    let kind = if non_passing.is_empty() {
        GateTerminalKind::Passed
    } else if state.phase() == GateRunPhase::Cancelling {
        GateTerminalKind::Cancelled
    } else if state.slots().iter().any(|slot| {
        slot.phase() == GateSlotPhase::Failed
            && slot
                .last_result()
                .is_none_or(|result| result.retry_permission() == RetryPermission::AfterRecovery)
    }) {
        GateTerminalKind::Indeterminate
    } else {
        GateTerminalKind::Failed
    };
    let digest = crate::canonical::terminal_digest(kind, &non_passing, state.state_digest());
    mutation::terminal(state, mutation::make_terminal(kind, non_passing, digest));
    Ok(())
}

pub fn dependencies_satisfied(plan: &GatePlan, state: &GateRunState, gate_id: GateId) -> bool {
    plan.gate(gate_id).is_some_and(|gate| {
        gate.dependencies().iter().all(|dependency| {
            state.slot(*dependency).is_some_and(|slot| slot.phase() == GateSlotPhase::Passed)
        })
    })
}

pub fn require_active(state: &GateRunState) -> Result<(), GateError> {
    if state.phase() == GateRunPhase::Active {
        Ok(())
    } else {
        Err(illegal("gate run is not active"))
    }
}

pub fn slot(state: &GateRunState, gate_id: GateId) -> Result<&crate::GateSlot, GateError> {
    state.slot(gate_id).ok_or_else(unknown_gate)
}

pub fn slot_mut(
    state: &mut GateRunState,
    gate_id: GateId,
) -> Result<&mut crate::GateSlot, GateError> {
    state.slot_mut(gate_id).ok_or_else(unknown_gate)
}

pub fn require_attempt(
    slot: &crate::GateSlot,
    phase: GateSlotPhase,
    execution_id: GateExecutionId,
) -> Result<(), GateError> {
    if slot.phase() == phase
        && slot.active_attempt().is_some_and(|active| active.execution_id() == execution_id)
    {
        Ok(())
    } else {
        Err(reject(
            GateRejection::IdentityMismatch,
            "gate phase or active execution identity differs",
        ))
    }
}

fn command_from_event(
    event: &GateEvent,
    expected_sequence: u64,
    previous: Option<peritus_types::EventId>,
) -> Result<GateCommand, GateError> {
    let kind = match event.kind() {
        GateEventKind::RunStarted { snapshot_digest } => {
            GateCommandKind::StartRun { snapshot_digest: *snapshot_digest }
        }
        GateEventKind::AttemptPrepared { gate_id, attempt } => {
            GateCommandKind::PrepareAttempt { gate_id: *gate_id, attempt: *attempt }
        }
        GateEventKind::AttemptDispatched { gate_id, execution_id } => {
            GateCommandKind::MarkDispatched { gate_id: *gate_id, execution_id: *execution_id }
        }
        GateEventKind::ResultObserved { gate_id, execution_id, result } => {
            GateCommandKind::ObserveResult {
                gate_id: *gate_id,
                execution_id: *execution_id,
                result: result.clone(),
            }
        }
        GateEventKind::RecoveryClassified { gate_id, execution_id, disposition } => {
            GateCommandKind::ClassifyRecovery {
                gate_id: *gate_id,
                execution_id: *execution_id,
                disposition: *disposition,
            }
        }
        GateEventKind::EvidencePublished { gate_id, execution_id, receipt } => {
            GateCommandKind::PublishEvidence {
                gate_id: *gate_id,
                execution_id: *execution_id,
                receipt: receipt.clone(),
            }
        }
        GateEventKind::CancellationStarted => GateCommandKind::BeginCancellation,
        GateEventKind::RunPaused { .. } => GateCommandKind::PauseRun,
        GateEventKind::RunResumed { .. } => GateCommandKind::ResumeRun,
        GateEventKind::RunFinalized => GateCommandKind::FinalizeRun,
    };
    GateCommand::new(
        event.command_id(),
        event.id(),
        event.run_id(),
        expected_sequence,
        previous,
        event.prior_state_digest(),
        event.revision(),
        kind,
    )
}

pub fn unknown_gate() -> GateError {
    reject(GateRejection::IdentityMismatch, "gate identity is absent from the exact plan")
}

pub fn illegal(detail: &'static str) -> GateError {
    reject(GateRejection::IllegalTransition, detail)
}

fn replay_error(detail: &'static str) -> GateError {
    reject(GateRejection::ReplayMismatch, detail)
}
