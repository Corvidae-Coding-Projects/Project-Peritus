//! Exact family routing and single-writer domain commits.

mod debugger;
mod evaluation;
mod evolution;
mod harness;

use peritus_app_protocol::AppErrorCode;
use peritus_codec::{CodecLimits, decode_message};
use peritus_journal::{CommittedBatch, SqliteJournal};
use peritus_types::{CommandId, EventId, RevisionTuple};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

/// Immutable facts copied from one already-validated A3 command binding.
pub struct DomainSubmission {
    command_id: CommandId,
    event_id: EventId,
    expected_previous_event: Option<EventId>,
    revision: RevisionTuple,
    family: u16,
    frame: Vec<u8>,
}

impl DomainSubmission {
    pub(crate) const fn new(
        command_id: CommandId,
        event_id: EventId,
        expected_previous_event: Option<EventId>,
        revision: RevisionTuple,
        family: u16,
        frame: Vec<u8>,
    ) -> Self {
        Self { command_id, event_id, expected_previous_event, revision, family, frame }
    }
}

/// Authoritative dispatch result retained by the application command ledger.
pub enum DomainOutcome {
    Committed(CommittedBatch),
    Rejected(AppErrorCode),
}

pub fn dispatch(
    journal: &mut SqliteJournal,
    submission: DomainSubmission,
) -> Result<DomainOutcome, DaemonError> {
    match submission.family {
        50 => gates(journal, &submission),
        53 => review(journal, &submission),
        70 => scheduler(journal, &submission),
        73 => collaboration(journal, &submission),
        76 => orchestrator(journal, &submission),
        79 => harness::dispatch(journal, &submission),
        82 => debugger::dispatch(journal, &submission),
        85 => evaluation::dispatch(journal, &submission),
        88 => evolution::dispatch_campaign(journal, &submission),
        91 => evolution::dispatch_pointer(journal, &submission),
        _ => Ok(DomainOutcome::Rejected(AppErrorCode::UnsupportedFamily)),
    }
}

fn gates(
    journal: &mut SqliteJournal,
    submission: &DomainSubmission,
) -> Result<DomainOutcome, DaemonError> {
    let frame = match decode_message::<peritus_gates::GateCommandFrame>(
        &submission.frame,
        CodecLimits::PRODUCTION,
    ) {
        Ok(frame) => frame,
        Err(_) => return malformed(),
    };
    let command = frame.into_command();
    if !binding_matches(
        submission,
        command.command_id(),
        command.event_id(),
        command.expected_previous_event(),
        command.revision(),
    ) {
        return binding_rejection();
    }
    if !matches!(
        command.kind(),
        peritus_gates::GateCommandKind::PauseRun | peritus_gates::GateCommandKind::ResumeRun
    ) {
        return semantic_rejection();
    }
    peritus_gates::commit_gate_lifecycle_transition(journal, &command)
        .map(DomainOutcome::Committed)
        .map_err(|error| domain_failure("commit gate lifecycle transition", error))
}

fn review(
    journal: &mut SqliteJournal,
    submission: &DomainSubmission,
) -> Result<DomainOutcome, DaemonError> {
    let frame = match decode_message::<peritus_review::ReviewCommandFrame>(
        &submission.frame,
        CodecLimits::PRODUCTION,
    ) {
        Ok(frame) => frame,
        Err(_) => return malformed(),
    };
    let command = frame.0;
    if !binding_matches(
        submission,
        command.command_id(),
        command.event_id(),
        command.expected_previous_event(),
        command.revision(),
    ) {
        return binding_rejection();
    }
    let replay = peritus_review::load_review_replay(journal, command.run_id())
        .map_err(|error| domain_failure("load review aggregate", error))?;
    let prior =
        replay.rebuild().map_err(|error| domain_failure("rebuild review aggregate", error))?;
    let transition = match prior.as_ref() {
        Some(state) => peritus_review::decide(state, &command),
        None => peritus_review::start(&command),
    };
    let Ok(transition) = transition else {
        return semantic_rejection();
    };
    peritus_review::commit_review_transition(journal, &command, &transition)
        .map(DomainOutcome::Committed)
        .map_err(|error| domain_failure("commit review transition", error))
}

fn scheduler(
    journal: &mut SqliteJournal,
    submission: &DomainSubmission,
) -> Result<DomainOutcome, DaemonError> {
    let frame = match decode_message::<peritus_scheduler::SchedulerCommandFrame>(
        &submission.frame,
        CodecLimits::PRODUCTION,
    ) {
        Ok(frame) => frame,
        Err(_) => return malformed(),
    };
    let command = frame.into_command();
    if !binding_matches(
        submission,
        command.command_id(),
        command.event_id(),
        command.expected_previous_event(),
        command.revision(),
    ) {
        return binding_rejection();
    }
    let replay = peritus_scheduler::load_scheduler_replay(journal, command.run_id())
        .map_err(|error| domain_failure("load scheduler aggregate", error))?;
    let prior =
        replay.rebuild().map_err(|error| domain_failure("rebuild scheduler aggregate", error))?;
    let transition = match prior.as_ref() {
        Some(state) => peritus_scheduler::decide(state, &command),
        None => peritus_scheduler::start(&command),
    };
    let Ok(transition) = transition else {
        return semantic_rejection();
    };
    peritus_scheduler::commit_scheduler_transition(journal, &command, &transition)
        .map(DomainOutcome::Committed)
        .map_err(|error| domain_failure("commit scheduler transition", error))
}

fn collaboration(
    journal: &mut SqliteJournal,
    submission: &DomainSubmission,
) -> Result<DomainOutcome, DaemonError> {
    let frame = match decode_message::<peritus_collaboration::CollaborationCommandFrame>(
        &submission.frame,
        CodecLimits::PRODUCTION,
    ) {
        Ok(frame) => frame,
        Err(_) => return malformed(),
    };
    let command = frame.0;
    if !binding_matches(
        submission,
        command.command_id(),
        command.event_id(),
        command.expected_previous_event(),
        command.revision(),
    ) {
        return binding_rejection();
    }
    let replay = peritus_collaboration::load_collaboration_replay(journal, command.run_id())
        .map_err(|error| domain_failure("load collaboration aggregate", error))?;
    let prior = replay
        .rebuild()
        .map_err(|error| domain_failure("rebuild collaboration aggregate", error))?;
    let transition = match prior.as_ref() {
        Some(state) => peritus_collaboration::decide(state, &command),
        None => peritus_collaboration::start(&command),
    };
    let Ok(transition) = transition else {
        return semantic_rejection();
    };
    peritus_collaboration::commit_collaboration_transition(journal, &command, &transition)
        .map(DomainOutcome::Committed)
        .map_err(|error| domain_failure("commit collaboration transition", error))
}

fn orchestrator(
    journal: &mut SqliteJournal,
    submission: &DomainSubmission,
) -> Result<DomainOutcome, DaemonError> {
    let frame = match decode_message::<peritus_orchestrator::OrchestratorCommandFrame>(
        &submission.frame,
        CodecLimits::PRODUCTION,
    ) {
        Ok(frame) => frame,
        Err(_) => return malformed(),
    };
    let command = frame.into_command();
    if !binding_matches(
        submission,
        command.command_id(),
        command.event_id(),
        command.expected_previous_event(),
        command.revision(),
    ) {
        return binding_rejection();
    }
    let replay = peritus_orchestrator::load_orchestrator_replay(journal, command.run_id())
        .map_err(|error| domain_failure("load orchestrator aggregate", error))?;
    let prior = replay
        .rebuild()
        .map_err(|error| domain_failure("rebuild orchestrator aggregate", error))?;
    let transition = match prior.as_ref() {
        Some(state) => peritus_orchestrator::decide(state, &command),
        None => peritus_orchestrator::start(&command),
    };
    let Ok(transition) = transition else {
        return semantic_rejection();
    };
    peritus_orchestrator::commit_orchestrator_transition(journal, &command, &transition)
        .map(DomainOutcome::Committed)
        .map_err(|error| domain_failure("commit orchestrator transition", error))
}

pub(super) fn binding_matches(
    submission: &DomainSubmission,
    command_id: CommandId,
    event_id: EventId,
    expected_previous_event: Option<EventId>,
    revision: RevisionTuple,
) -> bool {
    submission.command_id == command_id
        && submission.event_id == event_id
        && submission.expected_previous_event == expected_previous_event
        && submission.revision == revision
}

pub(super) fn binding_matches_without_revision(
    submission: &DomainSubmission,
    command_id: CommandId,
    event_id: EventId,
    expected_previous_event: Option<EventId>,
) -> bool {
    submission.command_id == command_id
        && submission.event_id == event_id
        && submission.expected_previous_event == expected_previous_event
}

pub(super) const fn malformed() -> Result<DomainOutcome, DaemonError> {
    Ok(DomainOutcome::Rejected(AppErrorCode::MalformedFrame))
}

pub(super) const fn binding_rejection() -> Result<DomainOutcome, DaemonError> {
    Ok(DomainOutcome::Rejected(AppErrorCode::CommandBindingMismatch))
}

pub(super) const fn semantic_rejection() -> Result<DomainOutcome, DaemonError> {
    Ok(DomainOutcome::Rejected(AppErrorCode::InvalidCommandFrame))
}

pub(super) fn domain_failure(
    operation: &'static str,
    error: impl std::error::Error + Send + Sync + 'static,
) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::RecoveryRequired,
        DaemonRecovery::Reconcile,
        operation,
        "authoritative domain persistence or replay failed",
        error,
    )
}
