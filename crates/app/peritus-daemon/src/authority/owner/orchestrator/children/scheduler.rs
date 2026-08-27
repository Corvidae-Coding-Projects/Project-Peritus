//! Native D3 scheduler pause and resume admission.

use peritus_journal::SqliteJournal;
use peritus_orchestrator::{ChildHead, DirectiveKind, OrchestratorState};
use peritus_scheduler::{
    SchedulerCommand, SchedulerCommandKind, SchedulerError, SchedulerRecoveryAction,
    SchedulerTransition,
};
use peritus_types::RunId;

use super::{
    ChildPredecessor, checked_lifecycle_head, child_ids, child_mismatch, unsupported_child,
};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery, outbox::OrchestratorDirectiveClaim};

const COMMAND_DOMAIN: &[u8] = b"peritus.g0.e0-scheduler-lifecycle.command.v1\0";
const EVENT_DOMAIN: &[u8] = b"peritus.g0.e0-scheduler-lifecycle.event.v1\0";

pub(super) fn admit_scheduler_directive(
    journal: &mut SqliteJournal,
    orchestrator: &OrchestratorState,
    claim: &OrchestratorDirectiveClaim,
) -> Result<(), DaemonError> {
    if claim.directive().kind() == DirectiveKind::CancelChildren {
        return Err(unsupported_child(
            "scheduler cancellation requires a native work root absent from the E0 directive",
        ));
    }
    let head = checked_lifecycle_head(orchestrator, claim)?;
    let cycle = orchestrator.current_quality_cycle();
    let run_id = cycle.scheduler_run_id();
    let replay = peritus_scheduler::load_scheduler_replay(journal, run_id)
        .map_err(|error| scheduler_error("load D3 scheduler child", error))?;
    let current = replay
        .rebuild()
        .map_err(|error| scheduler_error("rebuild D3 scheduler child", error))?
        .ok_or_else(|| child_mismatch("D3 scheduler directive names an absent run"))?;
    if current.run_id() != run_id
        || current.binding().scheduler_id() != cycle.scheduler_id()
        || current.binding().digest() != cycle.scheduler_binding_digest()
        || current.binding().revision() != claim.directive().revision()
    {
        return Err(child_mismatch("D3 scheduler binding differs from the E0 quality cycle"));
    }
    let predecessor = scheduler_predecessor(replay.events(), run_id, claim, head)?;
    let (command_id, event_id) = child_ids(
        COMMAND_DOMAIN,
        EVENT_DOMAIN,
        run_id,
        claim.directive().id(),
        "derive D3 scheduler lifecycle identities",
    )?;
    let kind = match claim.directive().kind() {
        DirectiveKind::PauseChildren => SchedulerCommandKind::PauseScheduler,
        DirectiveKind::ResumeChildren => SchedulerCommandKind::ResumeScheduler,
        _ => {
            return Err(child_mismatch(
                "scheduler destination received a non-lifecycle E0 directive",
            ));
        }
    };
    let command = SchedulerCommand::new(
        command_id,
        event_id,
        run_id,
        predecessor.sequence,
        Some(predecessor.event_id),
        predecessor.state_digest,
        claim.directive().revision(),
        kind,
    )
    .map_err(|error| scheduler_error("construct D3 scheduler lifecycle command", error))?;
    commit_scheduler_directive(journal, &command)
}

fn scheduler_predecessor(
    events: &[peritus_scheduler::SchedulerEvent],
    run_id: RunId,
    claim: &OrchestratorDirectiveClaim,
    head: ChildHead,
) -> Result<ChildPredecessor, DaemonError> {
    let head_event = exact_scheduler_head(events, run_id, claim, head)?;
    if claim.directive().kind() == DirectiveKind::PauseChildren {
        return Ok(predecessor_from_scheduler(head_event));
    }
    let pause_index = usize::try_from(head.sequence().get())
        .map_err(|_| child_mismatch("D3 scheduler sequence cannot address retained history"))?;
    let pause = events
        .get(pause_index)
        .ok_or_else(|| child_mismatch("D3 scheduler resume has no durable pause successor"))?;
    let exact = pause.run_id() == run_id
        && pause.sequence().get() == head.sequence().get().saturating_add(1)
        && pause.previous_event() == Some(head.last_event_id())
        && pause.prior_state_digest() == head.state_digest()
        && pause.revision() == claim.directive().revision()
        && matches!(pause.kind(), peritus_scheduler::SchedulerEventKind::SchedulerPaused);
    if !exact {
        return Err(child_mismatch(
            "D3 scheduler resume predecessor differs from the reconciled pause",
        ));
    }
    Ok(predecessor_from_scheduler(pause))
}

fn exact_scheduler_head<'a>(
    events: &'a [peritus_scheduler::SchedulerEvent],
    run_id: RunId,
    claim: &OrchestratorDirectiveClaim,
    head: ChildHead,
) -> Result<&'a peritus_scheduler::SchedulerEvent, DaemonError> {
    let index = usize::try_from(head.sequence().get().saturating_sub(1))
        .map_err(|_| child_mismatch("D3 scheduler sequence cannot address retained history"))?;
    let event =
        events.get(index).ok_or_else(|| child_mismatch("D3 scheduler child head is absent"))?;
    if event.run_id() != run_id
        || event.id() != head.last_event_id()
        || event.successor_state_digest() != head.state_digest()
        || event.revision() != claim.directive().revision()
    {
        return Err(child_mismatch("D3 scheduler child head differs from E0 reconciliation"));
    }
    Ok(event)
}

const fn predecessor_from_scheduler(event: &peritus_scheduler::SchedulerEvent) -> ChildPredecessor {
    ChildPredecessor {
        sequence: event.sequence().get(),
        event_id: event.id(),
        state_digest: event.successor_state_digest(),
    }
}

pub(in crate::authority::owner::orchestrator) fn commit_scheduler_directive(
    journal: &mut SqliteJournal,
    command: &SchedulerCommand,
) -> Result<(), DaemonError> {
    let replay = peritus_scheduler::load_scheduler_replay(journal, command.run_id())
        .map_err(|error| scheduler_error("reload D3 scheduler child", error))?;
    let current = replay
        .rebuild()
        .map_err(|error| scheduler_error("rebuild D3 scheduler child", error))?
        .ok_or_else(|| child_mismatch("D3 scheduler command names an absent run"))?;
    let transition = if command.expected_sequence() == current.sequence().get()
        && command.expected_previous_event() == Some(current.last_event_id())
        && command.prior_state_digest() == current.state_digest()
    {
        peritus_scheduler::decide(&current, command)
            .map_err(|error| scheduler_error("reduce D3 scheduler command", error))?
    } else {
        exact_scheduler_retry(replay.events(), &current, command)?
    };
    peritus_scheduler::commit_scheduler_transition(journal, command, &transition)
        .map_err(|error| scheduler_error("commit D3 scheduler command", error))?;
    Ok(())
}

fn exact_scheduler_retry(
    events: &[peritus_scheduler::SchedulerEvent],
    current: &peritus_scheduler::SchedulerState,
    command: &SchedulerCommand,
) -> Result<SchedulerTransition, DaemonError> {
    let (last, prefix) = events
        .split_last()
        .ok_or_else(|| child_mismatch("D3 scheduler retry has no durable event"))?;
    if last.id() != command.event_id() || last.command_id() != command.command_id() {
        return Err(child_mismatch("D3 scheduler command fence is stale"));
    }
    let predecessor = peritus_scheduler::replay(prefix)
        .map_err(|error| scheduler_error("replay D3 scheduler predecessor", error))?;
    let transition = peritus_scheduler::decide(&predecessor, command)
        .map_err(|error| scheduler_error("reconstruct D3 scheduler retry", error))?;
    if transition.event() != last || transition.state() != current {
        return Err(child_mismatch("D3 scheduler retry differs from durable state"));
    }
    Ok(transition)
}

fn scheduler_error(operation: &'static str, error: SchedulerError) -> DaemonError {
    let (code, recovery) = match error.recovery() {
        SchedulerRecoveryAction::Quarantine => {
            (DaemonErrorCode::CorruptState, DaemonRecovery::ReadOnly)
        }
        SchedulerRecoveryAction::RetryLater => (DaemonErrorCode::Worker, DaemonRecovery::Retry),
        SchedulerRecoveryAction::CorrectInput | SchedulerRecoveryAction::ReplayAggregate => {
            (DaemonErrorCode::RecoveryRequired, DaemonRecovery::Reconcile)
        }
    };
    let detail = error.detail().to_owned();
    DaemonError::with_source(code, recovery, operation, detail, error)
}
