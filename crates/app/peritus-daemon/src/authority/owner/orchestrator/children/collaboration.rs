//! Native D3 collaboration lifecycle, cancellation, and finalization admission.

use peritus_collaboration::{
    CollaborationCommand, CollaborationCommandKind, CollaborationError,
    CollaborationRecoveryAction, CollaborationState, CollaborationTransition,
};
use peritus_journal::SqliteJournal;
use peritus_orchestrator::{ChildHead, DirectiveKind, DirectivePayloadBinding, OrchestratorState};
use peritus_types::{CommandId, EventId, RunId};

use super::{
    ChildPredecessor, checked_lifecycle_head, checked_payload, child_ids, child_mismatch,
    unsupported_child,
};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery, outbox::OrchestratorDirectiveClaim};

const COMMAND_DOMAIN: &[u8] = b"peritus.g0.e0-collaboration.command.v1\0";
const EVENT_DOMAIN: &[u8] = b"peritus.g0.e0-collaboration.event.v1\0";

pub(super) fn admit_collaboration_directive(
    journal: &mut SqliteJournal,
    orchestrator: &OrchestratorState,
    claim: &OrchestratorDirectiveClaim,
) -> Result<(), DaemonError> {
    if matches!(
        claim.directive().kind(),
        DirectiveKind::StartWriter | DirectiveKind::StartReview | DirectiveKind::StartFixer
    ) {
        return Err(unsupported_child(
            "D3 handoff admission lacks the native scheduler work and reservation inputs",
        ));
    }
    let cycle = orchestrator.current_quality_cycle();
    let run_id = cycle.collaboration_run_id();
    let replay = peritus_collaboration::load_collaboration_replay(journal, run_id)
        .map_err(|error| collaboration_error("load D3 collaboration child", error))?;
    let current = replay
        .rebuild()
        .map_err(|error| collaboration_error("rebuild D3 collaboration child", error))?
        .ok_or_else(|| child_mismatch("D3 collaboration directive names an absent run"))?;
    if current.run_id() != run_id
        || current.binding().id() != cycle.collaboration_id()
        || current.binding().digest() != cycle.collaboration_binding_digest()
        || current.binding().scheduler_id() != cycle.scheduler_id()
        || current.binding().revision() != claim.directive().revision()
    {
        return Err(child_mismatch("D3 collaboration binding differs from the E0 quality cycle"));
    }
    let (command_id, event_id) = child_ids(
        COMMAND_DOMAIN,
        EVENT_DOMAIN,
        run_id,
        claim.directive().id(),
        "derive D3 collaboration identities",
    )?;
    let command = collaboration_command(
        orchestrator,
        claim,
        replay.events(),
        &current,
        command_id,
        event_id,
    )?;
    commit_collaboration_directive(journal, &command)
}

fn collaboration_command(
    orchestrator: &OrchestratorState,
    claim: &OrchestratorDirectiveClaim,
    events: &[peritus_collaboration::CollaborationEvent],
    current: &CollaborationState,
    command_id: CommandId,
    event_id: EventId,
) -> Result<CollaborationCommand, DaemonError> {
    let run_id = current.run_id();
    let (predecessor, kind) = match claim.directive().kind() {
        DirectiveKind::PauseChildren | DirectiveKind::ResumeChildren => {
            let head = checked_lifecycle_head(orchestrator, claim)?;
            let predecessor = collaboration_lifecycle_predecessor(events, run_id, claim, head)?;
            let owner = current.binding().root_assignment().owner();
            let kind = if claim.directive().kind() == DirectiveKind::PauseChildren {
                CollaborationCommandKind::Pause { requested_by: owner }
            } else {
                CollaborationCommandKind::Resume { requested_by: owner }
            };
            (predecessor, kind)
        }
        DirectiveKind::CancelChildren => {
            let cause = orchestrator
                .cancellation_cause()
                .ok_or_else(|| child_mismatch("E0 cancellation directive has no retained cause"))?;
            checked_payload(
                claim,
                DirectivePayloadBinding::Cancellation(cause),
                "E0 collaboration cancellation payload differs from the retained cause",
            )?;
            let predecessor = retry_or_current_predecessor(events, current, command_id, event_id)?;
            let root = current.binding().root_assignment();
            (
                predecessor,
                CollaborationCommandKind::CancelTask {
                    task_id: root.task_id(),
                    requested_by: root.owner(),
                    reason_digest: cause,
                },
            )
        }
        DirectiveKind::FinalizeChildren => {
            checked_payload(
                claim,
                DirectivePayloadBinding::QualityCycle(orchestrator.current_quality_cycle()),
                "E0 collaboration finalization payload differs from the current quality cycle",
            )?;
            (
                retry_or_current_predecessor(events, current, command_id, event_id)?,
                CollaborationCommandKind::Finalize,
            )
        }
        _ => {
            return Err(child_mismatch(
                "collaboration destination received an unsupported directive kind",
            ));
        }
    };
    CollaborationCommand::new(
        command_id,
        event_id,
        run_id,
        predecessor.sequence,
        Some(predecessor.event_id),
        predecessor.state_digest,
        claim.directive().revision(),
        kind,
    )
    .map_err(|error| collaboration_error("construct D3 collaboration command", error))
}

fn collaboration_lifecycle_predecessor(
    events: &[peritus_collaboration::CollaborationEvent],
    run_id: RunId,
    claim: &OrchestratorDirectiveClaim,
    head: ChildHead,
) -> Result<ChildPredecessor, DaemonError> {
    let head_event = exact_collaboration_head(events, run_id, claim, head)?;
    if claim.directive().kind() == DirectiveKind::PauseChildren {
        return Ok(predecessor_from_state_event(head_event));
    }
    let pause_index = usize::try_from(head.sequence().get())
        .map_err(|_| child_mismatch("D3 collaboration sequence cannot address history"))?;
    let pause = events
        .get(pause_index)
        .ok_or_else(|| child_mismatch("D3 collaboration resume has no durable pause successor"))?;
    let exact = pause.run_id() == run_id
        && pause.sequence().get() == head.sequence().get().saturating_add(1)
        && pause.previous_event() == Some(head.last_event_id())
        && pause.prior_state_digest() == head.state_digest()
        && pause.revision() == claim.directive().revision()
        && matches!(pause.kind(), peritus_collaboration::CollaborationEventKind::Paused { .. });
    if !exact {
        return Err(child_mismatch(
            "D3 collaboration resume predecessor differs from the reconciled pause",
        ));
    }
    Ok(predecessor_from_state_event(pause))
}

fn exact_collaboration_head<'a>(
    events: &'a [peritus_collaboration::CollaborationEvent],
    run_id: RunId,
    claim: &OrchestratorDirectiveClaim,
    head: ChildHead,
) -> Result<&'a peritus_collaboration::CollaborationEvent, DaemonError> {
    let index = usize::try_from(head.sequence().get().saturating_sub(1))
        .map_err(|_| child_mismatch("D3 collaboration sequence cannot address history"))?;
    let event =
        events.get(index).ok_or_else(|| child_mismatch("D3 collaboration child head is absent"))?;
    if event.run_id() != run_id
        || event.id() != head.last_event_id()
        || event.successor_state_digest() != head.state_digest()
        || event.revision() != claim.directive().revision()
    {
        return Err(child_mismatch("D3 collaboration child head differs from E0 reconciliation"));
    }
    Ok(event)
}

fn retry_or_current_predecessor(
    events: &[peritus_collaboration::CollaborationEvent],
    current: &CollaborationState,
    command_id: CommandId,
    event_id: EventId,
) -> Result<ChildPredecessor, DaemonError> {
    if let Some((last, prefix)) = events.split_last()
        && last.id() == event_id
        && last.command_id() == command_id
    {
        let predecessor = peritus_collaboration::replay(prefix)
            .map_err(|error| collaboration_error("replay D3 collaboration predecessor", error))?;
        return Ok(predecessor_from_state(&predecessor));
    }
    Ok(predecessor_from_state(current))
}

const fn predecessor_from_state_event(
    event: &peritus_collaboration::CollaborationEvent,
) -> ChildPredecessor {
    ChildPredecessor {
        sequence: event.sequence().get(),
        event_id: event.id(),
        state_digest: event.successor_state_digest(),
    }
}

const fn predecessor_from_state(state: &CollaborationState) -> ChildPredecessor {
    ChildPredecessor {
        sequence: state.sequence().get(),
        event_id: state.last_event_id(),
        state_digest: state.state_digest(),
    }
}

pub(in crate::authority::owner::orchestrator) fn commit_collaboration_directive(
    journal: &mut SqliteJournal,
    command: &CollaborationCommand,
) -> Result<(), DaemonError> {
    let replay = peritus_collaboration::load_collaboration_replay(journal, command.run_id())
        .map_err(|error| collaboration_error("reload D3 collaboration child", error))?;
    let current = replay
        .rebuild()
        .map_err(|error| collaboration_error("rebuild D3 collaboration child", error))?
        .ok_or_else(|| child_mismatch("D3 collaboration command names an absent run"))?;
    let transition = if command.expected_sequence() == current.sequence().get()
        && command.expected_previous_event() == Some(current.last_event_id())
        && command.prior_state_digest() == current.state_digest()
    {
        peritus_collaboration::decide(&current, command)
            .map_err(|error| collaboration_error("reduce D3 collaboration command", error))?
    } else {
        exact_collaboration_retry(replay.events(), &current, command)?
    };
    peritus_collaboration::commit_collaboration_transition(journal, command, &transition)
        .map_err(|error| collaboration_error("commit D3 collaboration command", error))?;
    Ok(())
}

fn exact_collaboration_retry(
    events: &[peritus_collaboration::CollaborationEvent],
    current: &CollaborationState,
    command: &CollaborationCommand,
) -> Result<CollaborationTransition, DaemonError> {
    let (last, prefix) = events
        .split_last()
        .ok_or_else(|| child_mismatch("D3 collaboration retry has no durable event"))?;
    if last.id() != command.event_id() || last.command_id() != command.command_id() {
        return Err(child_mismatch("D3 collaboration command fence is stale"));
    }
    let predecessor = peritus_collaboration::replay(prefix)
        .map_err(|error| collaboration_error("replay D3 collaboration predecessor", error))?;
    let transition = peritus_collaboration::decide(&predecessor, command)
        .map_err(|error| collaboration_error("reconstruct D3 collaboration retry", error))?;
    if transition.event() != last || transition.state() != current {
        return Err(child_mismatch("D3 collaboration retry differs from durable state"));
    }
    Ok(transition)
}

fn collaboration_error(operation: &'static str, error: CollaborationError) -> DaemonError {
    let (code, recovery) = match error.recovery() {
        CollaborationRecoveryAction::Quarantine => {
            (DaemonErrorCode::CorruptState, DaemonRecovery::ReadOnly)
        }
        CollaborationRecoveryAction::AwaitProgress => {
            (DaemonErrorCode::Worker, DaemonRecovery::Retry)
        }
        CollaborationRecoveryAction::CorrectInput
        | CollaborationRecoveryAction::ReplayAggregate => {
            (DaemonErrorCode::RecoveryRequired, DaemonRecovery::Reconcile)
        }
    };
    let detail = error.detail().to_owned();
    DaemonError::with_source(code, recovery, operation, detail, error)
}
