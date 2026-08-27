//! Native D1/D2 pause and resume admission.

use peritus_gates::{GateCommand, GateCommandKind, GateError, GateRecoveryAction};
use peritus_journal::SqliteJournal;
use peritus_orchestrator::{ChildHead, DirectiveDestination, DirectiveKind, OrchestratorState};
use peritus_review::{
    ReviewCommand, ReviewCommandKind, ReviewError, ReviewRecoveryAction, ReviewTransition,
};
use peritus_types::RunId;

use super::{ChildPredecessor, checked_lifecycle_head, child_ids, child_mismatch};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery, outbox::OrchestratorDirectiveClaim};

const GATE_COMMAND_DOMAIN: &[u8] = b"peritus.g0.e0-gates-lifecycle.command.v1\0";
const GATE_EVENT_DOMAIN: &[u8] = b"peritus.g0.e0-gates-lifecycle.event.v1\0";
const REVIEW_COMMAND_DOMAIN: &[u8] = b"peritus.g0.e0-review-lifecycle.command.v1\0";
const REVIEW_EVENT_DOMAIN: &[u8] = b"peritus.g0.e0-review-lifecycle.event.v1\0";

pub(super) fn admit_lifecycle_directive(
    journal: &mut SqliteJournal,
    orchestrator: &OrchestratorState,
    claim: &OrchestratorDirectiveClaim,
) -> Result<(), DaemonError> {
    let head = checked_lifecycle_head(orchestrator, claim)?;
    match claim.directive().destination() {
        DirectiveDestination::Gates => admit_gate_lifecycle(journal, orchestrator, claim, head),
        DirectiveDestination::Review => admit_review_lifecycle(journal, orchestrator, claim, head),
        _ => Err(child_mismatch("D1/D2 lifecycle handler received another destination")),
    }
}

fn admit_gate_lifecycle(
    journal: &mut SqliteJournal,
    orchestrator: &OrchestratorState,
    claim: &OrchestratorDirectiveClaim,
    head: ChildHead,
) -> Result<(), DaemonError> {
    let run_id = orchestrator.current_quality_cycle().gate_run_id();
    let replay = peritus_gates::load_gate_replay(journal, run_id)
        .map_err(|error| gate_error("load D1 lifecycle child", error))?;
    let predecessor = gate_predecessor(replay.events(), run_id, claim, head)?;
    let (command_id, event_id) = child_ids(
        GATE_COMMAND_DOMAIN,
        GATE_EVENT_DOMAIN,
        run_id,
        claim.directive().id(),
        "derive D1 lifecycle identities",
    )?;
    let kind = match claim.directive().kind() {
        DirectiveKind::PauseChildren => GateCommandKind::PauseRun,
        DirectiveKind::ResumeChildren => GateCommandKind::ResumeRun,
        _ => return Err(child_mismatch("D1 lifecycle directive kind changed after validation")),
    };
    let command = GateCommand::new(
        command_id,
        event_id,
        run_id,
        predecessor.sequence,
        Some(predecessor.event_id),
        predecessor.state_digest,
        claim.directive().revision(),
        kind,
    )
    .map_err(|error| gate_error("construct D1 lifecycle command", error))?;
    peritus_gates::commit_gate_lifecycle_transition(journal, &command)
        .map_err(|error| gate_error("commit D1 lifecycle command", error))?;
    Ok(())
}

fn admit_review_lifecycle(
    journal: &mut SqliteJournal,
    orchestrator: &OrchestratorState,
    claim: &OrchestratorDirectiveClaim,
    head: ChildHead,
) -> Result<(), DaemonError> {
    let run_id = orchestrator.binding().run_id();
    let replay = peritus_review::load_review_replay(journal, run_id)
        .map_err(|error| review_error("load D2 lifecycle child", error))?;
    let predecessor = review_predecessor(replay.events(), run_id, claim, head)?;
    let (command_id, event_id) = child_ids(
        REVIEW_COMMAND_DOMAIN,
        REVIEW_EVENT_DOMAIN,
        run_id,
        claim.directive().id(),
        "derive D2 lifecycle identities",
    )?;
    let kind = match claim.directive().kind() {
        DirectiveKind::PauseChildren => ReviewCommandKind::PauseRun,
        DirectiveKind::ResumeChildren => ReviewCommandKind::ResumeRun,
        _ => return Err(child_mismatch("D2 lifecycle directive kind changed after validation")),
    };
    let command = ReviewCommand::new(
        command_id,
        event_id,
        run_id,
        predecessor.sequence,
        Some(predecessor.event_id),
        predecessor.state_digest,
        claim.directive().revision(),
        kind,
    )
    .map_err(|error| review_error("construct D2 lifecycle command", error))?;
    commit_review_lifecycle(journal, &command)
}

fn gate_predecessor(
    events: &[peritus_gates::GateEvent],
    run_id: RunId,
    claim: &OrchestratorDirectiveClaim,
    head: ChildHead,
) -> Result<ChildPredecessor, DaemonError> {
    let head_event = exact_gate_head(events, run_id, claim, head)?;
    if claim.directive().kind() == DirectiveKind::PauseChildren {
        return Ok(predecessor_from_gate(head_event));
    }
    let pause_index = usize::try_from(head.sequence().get())
        .map_err(|_| child_mismatch("D1 child sequence cannot address retained history"))?;
    let pause = events
        .get(pause_index)
        .ok_or_else(|| child_mismatch("D1 resume has no durable pause successor"))?;
    let exact = pause.run_id() == run_id
        && pause.sequence().get() == head.sequence().get().saturating_add(1)
        && pause.previous_event() == Some(head.last_event_id())
        && pause.prior_state_digest() == head.state_digest()
        && pause.revision() == claim.directive().revision()
        && matches!(pause.kind(), peritus_gates::GateEventKind::RunPaused { .. });
    if !exact {
        return Err(child_mismatch("D1 resume predecessor differs from the reconciled pause"));
    }
    Ok(predecessor_from_gate(pause))
}

fn review_predecessor(
    events: &[peritus_review::ReviewEvent],
    run_id: RunId,
    claim: &OrchestratorDirectiveClaim,
    head: ChildHead,
) -> Result<ChildPredecessor, DaemonError> {
    let head_event = exact_review_head(events, run_id, claim, head)?;
    if claim.directive().kind() == DirectiveKind::PauseChildren {
        return Ok(predecessor_from_review(head_event));
    }
    let pause_index = usize::try_from(head.sequence().get())
        .map_err(|_| child_mismatch("D2 child sequence cannot address retained history"))?;
    let pause = events
        .get(pause_index)
        .ok_or_else(|| child_mismatch("D2 resume has no durable pause successor"))?;
    let exact = pause.run_id() == run_id
        && pause.sequence().get() == head.sequence().get().saturating_add(1)
        && pause.previous_event() == Some(head.last_event_id())
        && pause.prior_state_digest() == head.state_digest()
        && pause.revision() == claim.directive().revision()
        && matches!(pause.kind(), peritus_review::ReviewEventKind::RunPaused);
    if !exact {
        return Err(child_mismatch("D2 resume predecessor differs from the reconciled pause"));
    }
    Ok(predecessor_from_review(pause))
}

fn exact_gate_head<'a>(
    events: &'a [peritus_gates::GateEvent],
    run_id: RunId,
    claim: &OrchestratorDirectiveClaim,
    head: ChildHead,
) -> Result<&'a peritus_gates::GateEvent, DaemonError> {
    let index = usize::try_from(head.sequence().get().saturating_sub(1))
        .map_err(|_| child_mismatch("D1 child sequence cannot address retained history"))?;
    let event = events.get(index).ok_or_else(|| child_mismatch("D1 child head is absent"))?;
    if event.run_id() != run_id
        || event.id() != head.last_event_id()
        || event.successor_state_digest() != head.state_digest()
        || event.revision() != claim.directive().revision()
    {
        return Err(child_mismatch("D1 child head differs from E0 reconciliation"));
    }
    Ok(event)
}

fn exact_review_head<'a>(
    events: &'a [peritus_review::ReviewEvent],
    run_id: RunId,
    claim: &OrchestratorDirectiveClaim,
    head: ChildHead,
) -> Result<&'a peritus_review::ReviewEvent, DaemonError> {
    let index = usize::try_from(head.sequence().get().saturating_sub(1))
        .map_err(|_| child_mismatch("D2 child sequence cannot address retained history"))?;
    let event = events.get(index).ok_or_else(|| child_mismatch("D2 child head is absent"))?;
    if event.run_id() != run_id
        || event.id() != head.last_event_id()
        || event.successor_state_digest() != head.state_digest()
        || event.revision() != claim.directive().revision()
    {
        return Err(child_mismatch("D2 child head differs from E0 reconciliation"));
    }
    Ok(event)
}

const fn predecessor_from_gate(event: &peritus_gates::GateEvent) -> ChildPredecessor {
    ChildPredecessor {
        sequence: event.sequence().get(),
        event_id: event.id(),
        state_digest: event.successor_state_digest(),
    }
}

const fn predecessor_from_review(event: &peritus_review::ReviewEvent) -> ChildPredecessor {
    ChildPredecessor {
        sequence: event.sequence().get(),
        event_id: event.id(),
        state_digest: event.successor_state_digest(),
    }
}

pub(in crate::authority::owner::orchestrator) fn commit_review_lifecycle(
    journal: &mut SqliteJournal,
    command: &ReviewCommand,
) -> Result<(), DaemonError> {
    let replay = peritus_review::load_review_replay(journal, command.run_id())
        .map_err(|error| review_error("reload D2 lifecycle child", error))?;
    let current = replay
        .rebuild()
        .map_err(|error| review_error("rebuild D2 lifecycle child", error))?
        .ok_or_else(|| child_mismatch("D2 lifecycle command names an absent run"))?;
    let transition = if command.expected_sequence() == current.sequence().get()
        && command.expected_previous_event() == Some(current.last_event_id())
        && command.prior_state_digest() == current.state_digest()
    {
        peritus_review::decide(&current, command)
            .map_err(|error| review_error("reduce D2 lifecycle command", error))?
    } else {
        exact_review_retry(replay.events(), &current, command)?
    };
    peritus_review::commit_review_transition(journal, command, &transition)
        .map_err(|error| review_error("commit D2 lifecycle command", error))?;
    Ok(())
}

fn exact_review_retry(
    events: &[peritus_review::ReviewEvent],
    current: &peritus_review::ReviewRunState,
    command: &ReviewCommand,
) -> Result<ReviewTransition, DaemonError> {
    let (last, prefix) = events
        .split_last()
        .ok_or_else(|| child_mismatch("D2 lifecycle retry has no durable event"))?;
    if last.id() != command.event_id() || last.command_id() != command.command_id() {
        return Err(child_mismatch("D2 lifecycle command fence is stale"));
    }
    let predecessor = peritus_review::replay(prefix)
        .map_err(|error| review_error("replay D2 lifecycle predecessor", error))?;
    let transition = peritus_review::decide(&predecessor, command)
        .map_err(|error| review_error("reconstruct D2 lifecycle retry", error))?;
    if transition.event() != last || transition.state() != current {
        return Err(child_mismatch("D2 lifecycle retry differs from durable state"));
    }
    Ok(transition)
}

fn gate_error(operation: &'static str, error: GateError) -> DaemonError {
    let (code, recovery) = match error.recovery() {
        GateRecoveryAction::Quarantine => (DaemonErrorCode::CorruptState, DaemonRecovery::ReadOnly),
        GateRecoveryAction::FreshAction | GateRecoveryAction::RepublishEvidence => {
            (DaemonErrorCode::Worker, DaemonRecovery::Retry)
        }
        GateRecoveryAction::CorrectInput
        | GateRecoveryAction::ReplayAggregate
        | GateRecoveryAction::ReconcileAttempt => {
            (DaemonErrorCode::RecoveryRequired, DaemonRecovery::Reconcile)
        }
    };
    let detail = error.detail().to_owned();
    DaemonError::with_source(code, recovery, operation, detail, error)
}

fn review_error(operation: &'static str, error: ReviewError) -> DaemonError {
    let (code, recovery) = match error.recovery() {
        ReviewRecoveryAction::Quarantine => {
            (DaemonErrorCode::CorruptState, DaemonRecovery::ReadOnly)
        }
        ReviewRecoveryAction::ContinueReview => (DaemonErrorCode::Worker, DaemonRecovery::Retry),
        ReviewRecoveryAction::CorrectInput
        | ReviewRecoveryAction::ReplayAggregate
        | ReviewRecoveryAction::RequestAuthority
        | ReviewRecoveryAction::NeedsHuman => {
            (DaemonErrorCode::RecoveryRequired, DaemonRecovery::Reconcile)
        }
    };
    let detail = error.detail().to_owned();
    DaemonError::with_source(code, recovery, operation, detail, error)
}
