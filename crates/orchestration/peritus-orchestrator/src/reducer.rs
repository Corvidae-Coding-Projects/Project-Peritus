//! Pure deterministic E0 transitions and exact event replay.

mod apply;

use std::collections::BTreeSet;

use peritus_types::{EventSequence, Sha256Digest};

use crate::state::mutation;
use crate::{
    OrchestratorCommand, OrchestratorCommandKind, OrchestratorError, OrchestratorErrorKind,
    OrchestratorEvent, OrchestratorEventKind, OrchestratorPhase, OrchestratorRecoveryAction,
    OrchestratorState, OrchestratorTransition,
};

use apply::apply;

/// Starts one E0 run from the only legal genesis command.
///
/// # Errors
/// Rejects non-genesis fences, inconsistent binding/ownership/candidate/handoff, or non-start
/// commands.
pub fn start(command: &OrchestratorCommand) -> Result<OrchestratorTransition, OrchestratorError> {
    let OrchestratorCommandKind::Start { genesis } = command.kind() else {
        return Err(illegal("genesis command is not Start"));
    };
    let binding = genesis.binding();
    let candidate = genesis.candidate();
    let ownership = genesis.ownership();
    let writer_handoff = genesis.writer_handoff();
    binding.validate()?;
    candidate.validate(binding.limits())?;
    ownership.validate(binding.limits())?;
    writer_handoff.validate(binding.limits())?;
    validate_genesis(command, binding, candidate, ownership, writer_handoff)?;
    let sequence = EventSequence::first();
    let mut state = OrchestratorState::genesis(
        binding.clone(),
        ownership.clone(),
        candidate.clone(),
        writer_handoff.clone(),
        sequence,
        command.event_id(),
        command.command_id(),
    );
    let successor_digest = crate::canonical::state_digest(&state);
    mutation::set_state_digest(&mut state, successor_digest);
    state.validate()?;
    let event = OrchestratorEvent::from_wire(
        command.event_id(),
        command.command_id(),
        sequence,
        None,
        command.run_id(),
        command.revision(),
        Sha256Digest::new([0; 32]),
        successor_digest,
        OrchestratorEventKind::Started { genesis: genesis.clone() },
    );
    Ok(OrchestratorTransition::new(event, state))
}

/// Applies one fenced command to cloned state without performing external effects.
///
/// # Errors
/// Rejects stale fences, terminal state, reused identity, illegal phase/role/candidate movement,
/// false child authority, or any independent bound exhaustion.
pub fn decide(
    state: &OrchestratorState,
    command: &OrchestratorCommand,
) -> Result<OrchestratorTransition, OrchestratorError> {
    state.validate()?;
    validate_fences(state, command)?;
    let sequence = state
        .sequence()
        .checked_next()
        .map_err(|_| limit("orchestrator event sequence overflowed"))?;
    let mut successor = state.clone();
    let kind = apply(&mut successor, command.event_id(), command.kind())?;
    mutation::advance_cursor(&mut successor, sequence, command.event_id(), command.command_id());
    let successor_digest = crate::canonical::state_digest(&successor);
    mutation::set_state_digest(&mut successor, successor_digest);
    successor.validate()?;
    let event = OrchestratorEvent::from_wire(
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
    Ok(OrchestratorTransition::new(event, successor))
}

/// Reconstructs exact E0 state from genesis and canonical events.
///
/// # Errors
/// Rejects empty, duplicated, reordered, stale, tampered, or semantically illegal streams.
pub fn replay(events: &[OrchestratorEvent]) -> Result<OrchestratorState, OrchestratorError> {
    let first = events.first().ok_or_else(|| integrity("orchestrator replay is empty"))?;
    let first_command = command_from_event(first, 0, None)?;
    let first_transition = start(&first_command)?;
    if first_transition.event() != first {
        return Err(integrity("genesis event differs from deterministic reduction"));
    }
    let mut state = first_transition.into_state();
    let mut event_ids = BTreeSet::from([first.id()]);
    let mut command_ids = BTreeSet::from([first.command_id()]);
    for event in &events[1..] {
        if !event_ids.insert(event.id()) || !command_ids.insert(event.command_id()) {
            return Err(integrity("orchestrator event or command identity is duplicated"));
        }
        let command =
            command_from_event(event, state.sequence().get(), Some(state.last_event_id()))?;
        let transition = decide(&state, &command)?;
        if transition.event() != event {
            return Err(integrity("event differs from deterministic E0 reduction"));
        }
        state = transition.into_state();
    }
    Ok(state)
}

fn validate_genesis(
    command: &OrchestratorCommand,
    binding: &crate::OrchestratorBinding,
    candidate: &crate::CandidateBinding,
    ownership: &crate::RoleOwnership,
    handoff: &crate::Handoff,
) -> Result<(), OrchestratorError> {
    let exact = [
        command.run_id() == binding.run_id(),
        command.revision() == binding.initial_revision(),
        candidate.revision() == binding.initial_revision(),
        command.expected_sequence() == 0,
        command.expected_previous_event().is_none(),
        command.prior_state_digest() == Sha256Digest::new([0; 32]),
        handoff.kind() == crate::HandoffKind::Writer,
        handoff.source_actor() == ownership.service_actor(),
        handoff.destination_actor() == ownership.writer().actor(),
        handoff.candidate().materially_equal(candidate),
    ]
    .into_iter()
    .all(|part| part);
    if exact {
        Ok(())
    } else {
        Err(binding_error("genesis fences, identities, candidate, or writer handoff differ"))
    }
}

fn validate_fences(
    state: &OrchestratorState,
    command: &OrchestratorCommand,
) -> Result<(), OrchestratorError> {
    if state.phase() == OrchestratorPhase::Terminal {
        return Err(illegal("orchestrator aggregate is terminal and fenced closed"));
    }
    if u16::try_from(state.used_commands().len()).is_err() {
        return Err(limit("orchestrator command history reached its canonical bound"));
    }
    if state.binding().run_id() != command.run_id()
        || state.current_candidate().revision() != command.revision()
        || state.sequence().get() != command.expected_sequence()
        || command.expected_previous_event() != Some(state.last_event_id())
        || command.prior_state_digest() != state.state_digest()
        || state.used_commands().contains(&command.command_id())
        || matches!(command.kind(), OrchestratorCommandKind::Start { .. })
    {
        return Err(stale("command run, revision, predecessor, digest, or identity fence differs"));
    }
    Ok(())
}

#[allow(clippy::too_many_lines, reason = "closed event-to-command replay mapping stays contiguous")]
fn command_from_event(
    event: &OrchestratorEvent,
    expected_sequence: u64,
    previous: Option<peritus_types::EventId>,
) -> Result<OrchestratorCommand, OrchestratorError> {
    let kind = match event.kind() {
        OrchestratorEventKind::Started { genesis } => {
            OrchestratorCommandKind::Start { genesis: genesis.clone() }
        }
        OrchestratorEventKind::DirectivePublished { directive } => {
            OrchestratorCommandKind::PublishDirective { directive: directive.clone() }
        }
        OrchestratorEventKind::DirectiveAcknowledged { directive_id } => {
            OrchestratorCommandKind::AcknowledgeDirective { directive_id: *directive_id }
        }
        OrchestratorEventKind::HandoffActivated { activation } => {
            OrchestratorCommandKind::ObserveHandoffActivation { activation: activation.clone() }
        }
        OrchestratorEventKind::WriterObserved { observation, candidate, quality_cycle } => {
            OrchestratorCommandKind::ObserveWriter {
                observation: observation.clone(),
                candidate: candidate.clone(),
                quality_cycle: quality_cycle.clone(),
            }
        }
        OrchestratorEventKind::GatesObserved { observation, review_handoff } => {
            OrchestratorCommandKind::ObserveGates {
                observation: observation.clone(),
                review_handoff: review_handoff.clone(),
            }
        }
        OrchestratorEventKind::ReviewObserved { observation, fixer_handoff } => {
            OrchestratorCommandKind::ObserveReview {
                observation: observation.clone(),
                fixer_handoff: fixer_handoff.clone(),
            }
        }
        OrchestratorEventKind::FixerObserved { completion } => {
            OrchestratorCommandKind::ObserveFixer { completion: completion.clone() }
        }
        OrchestratorEventKind::RoleInfrastructureObserved { scheduler, collaboration } => {
            OrchestratorCommandKind::ObserveRoleInfrastructure {
                scheduler: scheduler.clone(),
                collaboration: collaboration.clone(),
            }
        }
        OrchestratorEventKind::CandidateAdvanced { quality_cycle, .. } => {
            OrchestratorCommandKind::AdvanceCandidate { quality_cycle: quality_cycle.clone() }
        }
        OrchestratorEventKind::AcceptanceCertificateRecorded { certificate } => {
            OrchestratorCommandKind::RecordAcceptanceCertificate {
                certificate: certificate.clone(),
            }
        }
        OrchestratorEventKind::KernelAcceptanceObserved { observation } => {
            OrchestratorCommandKind::ObserveKernelAcceptance { observation: *observation }
        }
        OrchestratorEventKind::Paused { reconciliation, .. } => {
            OrchestratorCommandKind::Pause { reconciliation: reconciliation.clone() }
        }
        OrchestratorEventKind::Resumed { reconciliation, .. } => {
            OrchestratorCommandKind::Resume { reconciliation: reconciliation.clone() }
        }
        OrchestratorEventKind::CancellationRequested { cause_digest } => {
            OrchestratorCommandKind::Cancel { cause_digest: *cause_digest }
        }
        OrchestratorEventKind::CancellationReconciled { observation } => {
            OrchestratorCommandKind::ReconcileCancellation { observation: observation.clone() }
        }
        OrchestratorEventKind::Rejected { terminal } => {
            OrchestratorCommandKind::Reject { cause_digest: terminal.cause_digest() }
        }
        OrchestratorEventKind::Failed { terminal } => {
            OrchestratorCommandKind::Fail { cause_digest: terminal.cause_digest() }
        }
        OrchestratorEventKind::Exhausted { terminal } => {
            OrchestratorCommandKind::Exhaust { cause_digest: terminal.cause_digest() }
        }
        OrchestratorEventKind::Finalized { .. } => OrchestratorCommandKind::Finalize,
    };
    OrchestratorCommand::new(
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

pub const fn illegal(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::InvalidTransition,
        OrchestratorRecoveryAction::CorrectInput,
        detail,
    )
}

pub const fn binding_error(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::BindingMismatch,
        OrchestratorRecoveryAction::Quarantine,
        detail,
    )
}

pub const fn stale(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::StaleState,
        OrchestratorRecoveryAction::Replay,
        detail,
    )
}

pub const fn limit(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::LimitExceeded,
        OrchestratorRecoveryAction::NeedsHuman,
        detail,
    )
}

pub const fn integrity(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::Integrity,
        OrchestratorRecoveryAction::Quarantine,
        detail,
    )
}
