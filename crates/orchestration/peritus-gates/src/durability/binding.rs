//! Cross-record semantic binding checks before C0 planning.

use crate::{GateCommand, GateCommandKind, GateError, GateEventKind, GateTransition};

pub fn validate_binding(
    command: &GateCommand,
    transition: &GateTransition,
) -> Result<(), GateError> {
    let event = transition.event();
    let state = transition.state();
    let mismatches = [
        command.event_id() != event.id(),
        command.command_id() != event.command_id(),
        command.run_id() != event.run_id(),
        command.run_id() != state.run_id(),
        command.expected_previous_event() != event.previous_event(),
        command.expected_sequence().checked_add(1) != Some(event.sequence().get()),
        command.revision() != event.revision(),
        command.revision() != state.revision(),
        command.prior_state_digest() != event.prior_state_digest(),
        event.successor_state_digest() != state.state_digest(),
        event.sequence() != state.sequence(),
        event.id() != state.last_event_id(),
        !event_matches_command(command.kind(), event.kind()),
    ];
    if mismatches.into_iter().any(core::convert::identity) {
        return Err(super::binding_error(
            "gate command, event, and checkpoint do not describe one transition",
        ));
    }
    Ok(())
}

fn event_matches_command(command: &GateCommandKind, event: &GateEventKind) -> bool {
    match (command, event) {
        (
            GateCommandKind::StartRun { snapshot_digest: command_snapshot },
            GateEventKind::RunStarted { snapshot_digest: event_snapshot },
        ) => command_snapshot == event_snapshot,
        (
            GateCommandKind::PrepareAttempt { gate_id: command_gate, attempt: command_attempt },
            GateEventKind::AttemptPrepared { gate_id: event_gate, attempt: event_attempt },
        ) => command_gate == event_gate && command_attempt == event_attempt,
        (
            GateCommandKind::MarkDispatched {
                gate_id: command_gate,
                execution_id: command_execution,
            },
            GateEventKind::AttemptDispatched { gate_id: event_gate, execution_id: event_execution },
        ) => command_gate == event_gate && command_execution == event_execution,
        (
            GateCommandKind::ObserveResult {
                gate_id: command_gate,
                execution_id: command_execution,
                result: command_result,
            },
            GateEventKind::ResultObserved {
                gate_id: event_gate,
                execution_id: event_execution,
                result: event_result,
            },
        ) => {
            command_gate == event_gate
                && command_execution == event_execution
                && command_result == event_result
        }
        (
            GateCommandKind::ClassifyRecovery {
                gate_id: command_gate,
                execution_id: command_execution,
                disposition: command_disposition,
            },
            GateEventKind::RecoveryClassified {
                gate_id: event_gate,
                execution_id: event_execution,
                disposition: event_disposition,
            },
        ) => {
            command_gate == event_gate
                && command_execution == event_execution
                && command_disposition == event_disposition
        }
        (
            GateCommandKind::PublishEvidence {
                gate_id: command_gate,
                execution_id: command_execution,
                receipt: command_receipt,
            },
            GateEventKind::EvidencePublished {
                gate_id: event_gate,
                execution_id: event_execution,
                receipt: event_receipt,
            },
        ) => {
            command_gate == event_gate
                && command_execution == event_execution
                && command_receipt == event_receipt
        }
        (GateCommandKind::BeginCancellation, GateEventKind::CancellationStarted)
        | (GateCommandKind::FinalizeRun, GateEventKind::RunFinalized) => true,
        _ => false,
    }
}
