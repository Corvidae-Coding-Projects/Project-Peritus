//! Atomic family-86 event, family-87 checkpoint, artifact, and outbox persistence.

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_journal::{
    AppendRequest, ArtifactDependency, CommandResolution, CommittedBatch, EventDraft, ExactFrame,
    HeadExpectation, OutboxAcknowledgement, OutboxDraft, SqliteJournal, StateInstall,
};
use peritus_types::EventSequence;

use crate::{
    EvaluationCommand, EvaluationCommandKind, EvaluationDirectiveClaim, EvaluationError,
    EvaluationErrorKind, EvaluationEventKind, EvaluationOperation, EvaluationRecovery,
    EvaluationState, EvaluationTransition, ExecutionDirective, ExecutionDirectiveKind,
    PUBLICATION_DESTINATION, PublicationDirective, RolloutStatus, SCHEDULE_DESTINATION,
    ScheduleDirective, ScheduleDirectiveKind,
    wire::{EvaluationCommandFrame, EvaluationEventFrame, EvaluationStateFrame},
};

use super::{
    EVALUATION_STATE_NAMESPACE, EXECUTION_DESTINATION, binding, evaluation_aggregate_key,
    evaluation_state_key,
};

const OUTBOX_MAX_DELIVERY_ATTEMPTS: u16 = 16;

/// Atomically appends an ordinary transition and its complete checkpoint.
///
/// # Errors
/// Rejects invalid bindings, stale C0 fences, missing artifacts, or journal failures.
pub fn commit_evaluation_transition(
    journal: &mut SqliteJournal,
    command: &EvaluationCommand,
    transition: &EvaluationTransition,
) -> Result<CommittedBatch, EvaluationError> {
    commit(journal, command, transition, CommitMode::Ordinary)
}

/// Commits a claimed schedule/execution attempt start before external I/O.
///
/// # Errors
/// Rejects a claim that differs from the exact transition or any C0 commit failure.
pub fn commit_evaluation_claimed_transition(
    journal: &mut SqliteJournal,
    command: &EvaluationCommand,
    transition: &EvaluationTransition,
    claim: impl Into<EvaluationDirectiveClaim>,
) -> Result<CommittedBatch, EvaluationError> {
    commit(journal, command, transition, CommitMode::Claimed(claim.into()))
}

/// Atomically commits an effect result and acknowledges its exact claim.
///
/// # Errors
/// Rejects a mismatched claim, stale fence, invalid transition, or C0 commit failure.
pub fn commit_evaluation_settlement(
    journal: &mut SqliteJournal,
    command: &EvaluationCommand,
    transition: &EvaluationTransition,
    claim: impl Into<EvaluationDirectiveClaim>,
) -> Result<CommittedBatch, EvaluationError> {
    commit(journal, command, transition, CommitMode::Settlement(claim.into()))
}

enum CommitMode {
    Ordinary,
    Claimed(EvaluationDirectiveClaim),
    Settlement(EvaluationDirectiveClaim),
}

fn commit(
    journal: &mut SqliteJournal,
    command: &EvaluationCommand,
    transition: &EvaluationTransition,
    mode: CommitMode,
) -> Result<CommittedBatch, EvaluationError> {
    binding::validate(command, transition)?;
    validate_mode(command, transition.state(), &mode)?;
    let event = transition.event();
    let state = transition.state();
    let aggregate = evaluation_aggregate_key(command.campaign_id())?;
    let state_key = evaluation_state_key(command.campaign_id());
    let command_bytes = encode_message(
        &EvaluationCommandFrame::from_command(command).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    let event_bytes = encode_message(
        &EvaluationEventFrame::from_event(event).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    let state_bytes =
        encode_message(&EvaluationStateFrame::from_state(state), CodecLimits::PRODUCTION)
            .map_err(codec)?;
    let base_digest = peritus_codec::sha256(&command_bytes);
    let request_digest = match &mode {
        CommitMode::Ordinary => base_digest,
        CommitMode::Claimed(claim) => {
            bound_digest(b"PERITUS-E3-OUTBOX-CLAIM\0", base_digest, claim)?
        }
        CommitMode::Settlement(claim) => {
            bound_digest(b"PERITUS-C0-OUTBOX-ACKNOWLEDGEMENTS\0", base_digest, claim)?
        }
    };
    if let Some(batch) = resolve_existing(
        journal,
        command,
        aggregate,
        &state_key,
        &event_bytes,
        state,
        request_digest,
    )? {
        return Ok(batch);
    }
    let head = journal.head(aggregate).map_err(journal_error)?;
    let current =
        journal.state_record(EVALUATION_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    validate_current(command, head, current.as_ref())?;
    let draft = EventDraft::new(
        aggregate,
        EventSequence::new(event.sequence())
            .map_err(|_| binding::binding("event sequence is zero"))?,
        event.id(),
        event.previous_event(),
        ExactFrame::new(event_bytes).map_err(journal_error)?,
        peritus_evidence::revision_digest(state.revision()),
        Vec::new(),
    )
    .map_err(journal_error)?;
    let install = StateInstall::new(
        EVALUATION_STATE_NAMESPACE,
        state_key,
        current.as_ref().map(peritus_journal::DurableStateRecord::revision),
        state.sequence(),
        state_bytes,
    )
    .map_err(journal_error)?;
    let expectation = head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
    let dependencies = artifact_dependencies(event.kind());
    let outbox = transition_outbox(command, state)?;
    let request_base_digest = match mode {
        CommitMode::Claimed(_) => request_digest,
        CommitMode::Ordinary | CommitMode::Settlement(_) => base_digest,
    };
    let request = AppendRequest::new(
        journal.store_id(),
        command.command_id(),
        request_base_digest,
        vec![expectation],
        vec![draft],
        vec![install],
        dependencies,
        None,
        None,
        outbox,
    );
    let request = if let CommitMode::Settlement(claim) = mode {
        request
            .with_outbox_acknowledgements(vec![
                OutboxAcknowledgement::new(claim.id()?, claim.fence()).map_err(journal_error)?,
            ])
            .map_err(journal_error)?
    } else {
        request
    };
    journal.append(request.plan().map_err(journal_error)?).map_err(journal_error)
}

fn transition_outbox(
    command: &EvaluationCommand,
    state: &EvaluationState,
) -> Result<Vec<OutboxDraft>, EvaluationError> {
    match command.kind() {
        EvaluationCommandKind::RequestSchedule { rollout_id, work } => {
            let directive =
                ScheduleDirective::submit(command.campaign_id(), *rollout_id, work.clone())?;
            Ok(vec![outbox(
                directive.outbox_id()?,
                SCHEDULE_DESTINATION,
                directive.canonical_bytes()?,
            )?])
        }
        EvaluationCommandKind::RecordSchedule { rollout_id, .. } => {
            let progress = state
                .rollout(*rollout_id)
                .ok_or_else(|| binding::binding("scheduled rollout vanished"))?;
            let directive = ExecutionDirective::execute(
                command.campaign_id(),
                *rollout_id,
                progress.binding().request_digest(),
            );
            Ok(vec![outbox(
                directive.outbox_id()?,
                EXECUTION_DESTINATION,
                directive.canonical_bytes()?,
            )?])
        }
        EvaluationCommandKind::CompleteReport { report } => {
            let directive = PublicationDirective::new(command.campaign_id(), *report);
            Ok(vec![outbox(
                directive.outbox_id()?,
                PUBLICATION_DESTINATION,
                directive.canonical_bytes()?,
            )?])
        }
        _ => {
            // Cancellation reuses the rollout's one outstanding schedule/execution claim. Emitting
            // a second directive would leave that original claim unaccounted.
            Ok(Vec::new())
        }
    }
}

fn artifact_dependencies(kind: &EvaluationEventKind) -> Vec<ArtifactDependency> {
    let EvaluationEventKind::Accepted(kind) = kind;
    let mut dependencies = match kind {
        EvaluationCommandKind::CreateCampaign { dataset_artifact, profile_artifact, .. } => vec![
            ArtifactDependency::new(dataset_artifact.sha256()),
            ArtifactDependency::new(profile_artifact.sha256()),
        ],
        EvaluationCommandKind::RecordPlanBatch { batch, .. } => {
            vec![ArtifactDependency::new(batch.artifact().sha256())]
        }
        EvaluationCommandKind::CompletePlan { plan } => {
            vec![ArtifactDependency::new(plan.root().sha256())]
        }
        EvaluationCommandKind::SettleRollout { terminal, .. } => {
            vec![ArtifactDependency::new(terminal.artifact().sha256())]
        }
        EvaluationCommandKind::CompleteAnalysis { artifact, .. } => {
            vec![ArtifactDependency::new(artifact.sha256())]
        }
        EvaluationCommandKind::CompleteReport { report } => {
            vec![ArtifactDependency::new(report.artifact().sha256())]
        }
        _ => Vec::new(),
    };
    dependencies.sort_unstable();
    dependencies.dedup();
    dependencies
}

fn validate_mode(
    command: &EvaluationCommand,
    state: &EvaluationState,
    mode: &CommitMode,
) -> Result<(), EvaluationError> {
    match mode {
        CommitMode::Ordinary => match command.kind() {
            EvaluationCommandKind::RecordSchedule { .. }
            | EvaluationCommandKind::StartRollout { .. }
            | EvaluationCommandKind::RetainRetryableAttempt { .. }
            | EvaluationCommandKind::SettleRollout { .. }
            | EvaluationCommandKind::SettleCancellation { .. }
            | EvaluationCommandKind::RecordPublication { .. } => {
                Err(binding::binding("effect transition requires its exact claimed directive"))
            }
            _ => Ok(()),
        },
        CommitMode::Claimed(claim) => match (command.kind(), claim) {
            (
                EvaluationCommandKind::StartRollout { rollout_id, .. },
                EvaluationDirectiveClaim::Execution(value),
            ) if value.directive().campaign_id() == command.campaign_id()
                && value.directive().rollout_id() == *rollout_id
                && matches!(value.directive().kind(), ExecutionDirectiveKind::Execute { .. })
                && matches!(
                    state.rollout(*rollout_id).map(crate::RolloutProgress::status),
                    Some(RolloutStatus::Running { .. })
                ) =>
            {
                Ok(())
            }
            _ => Err(binding::binding("claimed directive differs from pre-effect transition")),
        },
        CommitMode::Settlement(claim) => validate_settlement(command, claim),
    }
}

fn validate_settlement(
    command: &EvaluationCommand,
    claim: &EvaluationDirectiveClaim,
) -> Result<(), EvaluationError> {
    let matches = match (command.kind(), claim) {
        (
            EvaluationCommandKind::RecordSchedule { rollout_id, .. },
            EvaluationDirectiveClaim::Schedule(value),
        ) => {
            value.directive().campaign_id() == command.campaign_id()
                && value.directive().rollout_id() == *rollout_id
                && matches!(value.directive().kind(), ScheduleDirectiveKind::Submit(_))
        }
        (
            EvaluationCommandKind::RetainRetryableAttempt { rollout_id, .. }
            | EvaluationCommandKind::SettleRollout { rollout_id, .. },
            EvaluationDirectiveClaim::Execution(value),
        ) => {
            value.directive().campaign_id() == command.campaign_id()
                && value.directive().rollout_id() == *rollout_id
                && matches!(value.directive().kind(), ExecutionDirectiveKind::Execute { .. })
        }
        (
            EvaluationCommandKind::RecordPublication { publication },
            EvaluationDirectiveClaim::Publication(value),
        ) => {
            value.directive().campaign_id() == command.campaign_id()
                && value.directive().report().id() == publication.report_id()
        }
        (
            EvaluationCommandKind::SettleCancellation { rollout_id, .. },
            EvaluationDirectiveClaim::Schedule(value),
        ) => {
            value.directive().campaign_id() == command.campaign_id()
                && value.directive().rollout_id() == *rollout_id
                && matches!(
                    value.directive().kind(),
                    ScheduleDirectiveKind::Submit(_) | ScheduleDirectiveKind::Cancel(_)
                )
        }
        (
            EvaluationCommandKind::SettleCancellation { rollout_id, .. },
            EvaluationDirectiveClaim::Execution(value),
        ) => {
            value.directive().campaign_id() == command.campaign_id()
                && value.directive().rollout_id() == *rollout_id
                && matches!(
                    value.directive().kind(),
                    ExecutionDirectiveKind::Execute { .. } | ExecutionDirectiveKind::Cancel
                )
        }
        _ => false,
    };
    if matches { Ok(()) } else { Err(binding::binding("claim differs from effect settlement")) }
}

fn validate_current(
    command: &EvaluationCommand,
    head: Option<peritus_journal::AggregateHead>,
    current: Option<&peritus_journal::DurableStateRecord>,
) -> Result<(), EvaluationError> {
    if head.is_some() != current.is_some() {
        return Err(recovery("evaluation journal head/checkpoint presence differs"));
    }
    match head {
        None if command.expected_sequence() != 0 => {
            return Err(binding::binding("evaluation genesis expects an existing C0 head"));
        }
        Some(observed)
            if observed.sequence().get() != command.expected_sequence()
                || Some(observed.event_id()) != command.expected_previous_event() =>
        {
            return Err(binding::binding("command fence differs from the C0 head"));
        }
        _ => {}
    }
    if current.is_some_and(|record| record.revision() != command.expected_sequence()) {
        return Err(recovery("evaluation checkpoint revision differs from C0 head"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, reason = "complete idempotency evidence remains explicit")]
fn resolve_existing(
    journal: &SqliteJournal,
    command: &EvaluationCommand,
    aggregate: peritus_journal::AggregateKey,
    state_key: &[u8],
    event_bytes: &[u8],
    state: &EvaluationState,
    request_digest: peritus_types::Sha256Digest,
) -> Result<Option<CommittedBatch>, EvaluationError> {
    let batch = match journal
        .resolve_command(command.command_id(), request_digest)
        .map_err(journal_error)?
    {
        CommandResolution::Committed(batch) => batch,
        CommandResolution::Conflict { .. } => {
            return Err(binding::binding("command identity was committed with another request"));
        }
        CommandResolution::DefinitelyAbsent => return Ok(None),
    };
    let checkpoint = journal
        .state_record(EVALUATION_STATE_NAMESPACE, state_key)
        .map_err(journal_error)?
        .ok_or_else(|| recovery("resolved command has no evaluation checkpoint"))?;
    if batch.records().len() != 1
        || batch.records()[0].frame_bytes() != event_bytes
        || batch.records()[0].aggregate() != aggregate
    {
        return Err(recovery("resolved command differs from its exact evaluation event"));
    }
    let observed =
        decode_message::<EvaluationStateFrame>(checkpoint.bytes(), CodecLimits::PRODUCTION)
            .map_err(codec)?;
    if checkpoint.revision() == state.sequence() && observed.matches_state(state) {
        return Ok(Some(batch));
    }
    Err(recovery("resolved evaluation checkpoint differs from exact successor"))
}

fn outbox(
    id: peritus_journal::OutboxId,
    destination: &str,
    payload: Vec<u8>,
) -> Result<OutboxDraft, EvaluationError> {
    OutboxDraft::new(id, destination.to_owned(), payload, OUTBOX_MAX_DELIVERY_ATTEMPTS)
        .map_err(journal_error)
}
fn bound_digest(
    domain: &[u8],
    base: peritus_types::Sha256Digest,
    claim: &EvaluationDirectiveClaim,
) -> Result<peritus_types::Sha256Digest, EvaluationError> {
    let mut bytes = Vec::with_capacity(domain.len() + 32 + 16 + 8);
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(base.as_bytes());
    bytes.extend_from_slice(claim.id()?.as_bytes());
    bytes.extend_from_slice(&claim.fence().to_be_bytes());
    Ok(peritus_codec::sha256(&bytes))
}
fn codec(_: impl core::fmt::Display) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Corruption,
        EvaluationOperation::Codec,
        EvaluationRecovery::Quarantine,
        "evaluation C0 frame violates canonical protocol",
    )
}
#[allow(
    clippy::needless_pass_by_value,
    reason = "map_err transfers ownership while this redaction boundary retains only the stable category"
)]
fn journal_error(error: peritus_journal::JournalError) -> EvaluationError {
    let detail = match error.kind() {
        peritus_journal::JournalErrorKind::MissingArtifact => {
            "C0 rejected a missing or inactive evaluation artifact dependency"
        }
        peritus_journal::JournalErrorKind::IdempotencyConflict => {
            "C0 rejected a conflicting evaluation command identity"
        }
        peritus_journal::JournalErrorKind::StaleHead => {
            "C0 rejected a stale evaluation aggregate head"
        }
        peritus_journal::JournalErrorKind::UnsupportedSchema => {
            "C0 evaluation schema version is unsupported"
        }
        peritus_journal::JournalErrorKind::InvalidInput => "C0 rejected invalid evaluation input",
        peritus_journal::JournalErrorKind::EmptyBatch => "C0 rejected an empty evaluation batch",
        peritus_journal::JournalErrorKind::DuplicateIdentity => {
            "C0 rejected duplicate evaluation identities"
        }
        peritus_journal::JournalErrorKind::NonCanonicalOrder => {
            "C0 rejected noncanonical evaluation ordering"
        }
        peritus_journal::JournalErrorKind::SequenceOverflow => "C0 evaluation sequence overflowed",
        peritus_journal::JournalErrorKind::StaleAuthorityEpoch => {
            "C0 authority epoch was stale during evaluation commit"
        }
        peritus_journal::JournalErrorKind::StaleRegistry => {
            "C0 registry was stale during evaluation commit"
        }
        peritus_journal::JournalErrorKind::Busy => "C0 was busy during evaluation commit",
        peritus_journal::JournalErrorKind::ReadOnly => "C0 is read-only for evaluation commit",
        peritus_journal::JournalErrorKind::IndeterminateCommit => {
            "C0 evaluation commit outcome is indeterminate"
        }
        peritus_journal::JournalErrorKind::CorruptJournal => {
            "C0 journal is corrupt during evaluation commit"
        }
        peritus_journal::JournalErrorKind::NotFound => {
            "C0 dependency was not found during evaluation commit"
        }
        peritus_journal::JournalErrorKind::Storage => "C0 storage failed during evaluation commit",
    };
    EvaluationError::new(
        EvaluationErrorKind::Journal,
        EvaluationOperation::Commit,
        EvaluationRecovery::Replay,
        detail,
    )
}
const fn recovery(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Recovery,
        EvaluationOperation::Recover,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
