//! Atomic family-86 event, family-87 checkpoint, artifact, and outbox persistence.

use peritus_codec::{CodecLimits, encode_message};
use peritus_journal::{
    AppendRequest, ArtifactDependency, CommandResolution, CommittedBatch, EventDraft, ExactFrame,
    HeadExpectation, OutboxAcknowledgement, OutboxDraft, SqliteJournal, StateInstall,
};
use peritus_types::{CommandId, EventSequence};

use crate::{
    EvaluationCommand, EvaluationCommandKind, EvaluationDirectiveClaim, EvaluationError,
    EvaluationErrorKind, EvaluationEvent, EvaluationEventKind, EvaluationOperation,
    EvaluationRecovery, EvaluationState, EvaluationTransition, ExecutionDirective,
    ExecutionDirectiveKind, PUBLICATION_DESTINATION, PublicationDirective, RolloutStatus,
    SCHEDULE_DESTINATION, ScheduleDirective, ScheduleDirectiveKind,
    wire::{EvaluationCommandFrame, EvaluationEventFrame},
};

use super::{
    CommittedEvaluationOperation, EVALUATION_RECEIPT_NAMESPACE, EVALUATION_STATE_NAMESPACE,
    EXECUTION_DESTINATION, EvaluationCommitMode, EvaluationOperationReceipt, binding,
    evaluation_aggregate_key, evaluation_state_key, load_evaluation_replay,
    receipt::{claim_receipt, receipt_key},
};

/// Atomically appends an ordinary transition and its complete checkpoint.
///
/// # Errors
/// Rejects invalid bindings, stale C0 fences, missing artifacts, or journal failures.
pub fn commit_evaluation_transition(
    journal: &mut SqliteJournal,
    command: &EvaluationCommand,
    transition: &EvaluationTransition,
) -> Result<CommittedEvaluationOperation, EvaluationError> {
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
) -> Result<CommittedEvaluationOperation, EvaluationError> {
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
) -> Result<CommittedEvaluationOperation, EvaluationError> {
    commit(journal, command, transition, CommitMode::Settlement(claim.into()))
}

enum CommitMode {
    Ordinary,
    Claimed(EvaluationDirectiveClaim),
    Settlement(EvaluationDirectiveClaim),
}

impl CommitMode {
    const fn receipt_mode(&self) -> EvaluationCommitMode {
        match self {
            Self::Ordinary => EvaluationCommitMode::Ordinary,
            Self::Claimed(_) => EvaluationCommitMode::Claimed,
            Self::Settlement(_) => EvaluationCommitMode::Settlement,
        }
    }

    const fn claim(&self) -> Option<&EvaluationDirectiveClaim> {
        match self {
            Self::Ordinary => None,
            Self::Claimed(claim) | Self::Settlement(claim) => Some(claim),
        }
    }

    const fn is_claim_bound(&self) -> bool {
        !matches!(self, Self::Ordinary)
    }
}

fn commit(
    journal: &mut SqliteJournal,
    command: &EvaluationCommand,
    transition: &EvaluationTransition,
    mode: CommitMode,
) -> Result<CommittedEvaluationOperation, EvaluationError> {
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
    let receipt = operation_receipt(
        command,
        event,
        peritus_codec::sha256(&event_bytes),
        base_digest,
        request_digest,
        &mode,
    )?;
    if let Some(operation) = resolve_existing(
        journal,
        command,
        aggregate,
        event,
        &event_bytes,
        state,
        request_digest,
        receipt,
        &mode,
    )? {
        return Ok(operation);
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
    let mut installs = super::checkpoint::installs(
        journal,
        &state_key,
        current.as_ref().map(peritus_journal::DurableStateRecord::revision),
        state,
    )?;
    installs.push(
        StateInstall::new(
            EVALUATION_RECEIPT_NAMESPACE,
            receipt_key(command.command_id()),
            None,
            1,
            receipt.canonical_bytes()?,
        )
        .map_err(journal_error)?,
    );
    let expectation = head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
    let dependencies = artifact_dependencies(event.kind());
    let outbox = transition_outbox(command, state)?;
    let request_base_digest = match &mode {
        CommitMode::Claimed(_) => request_digest,
        CommitMode::Ordinary | CommitMode::Settlement(_) => base_digest,
    };
    let request = AppendRequest::new(
        journal.store_id(),
        command.command_id(),
        request_base_digest,
        vec![expectation],
        vec![draft],
        installs,
        dependencies,
        None,
        None,
        outbox,
    );
    let request = if let CommitMode::Settlement(claim) = &mode {
        request
            .with_outbox_acknowledgements(vec![
                OutboxAcknowledgement::new(claim.outbox_id()?, claim.fence())
                    .map_err(journal_error)?,
            ])
            .map_err(journal_error)?
    } else {
        request
    };
    let _accepted =
        journal.append(request.plan().map_err(journal_error)?).map_err(journal_error)?;
    load_evaluation_operation(journal, command.campaign_id(), command.command_id())?
        .ok_or_else(|| recovery("accepted evaluation operation has no durable receipt"))
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
            let attempt = progress
                .attempts_retained()
                .checked_add(1)
                .ok_or_else(|| binding::binding("scheduled rollout attempt overflowed"))?;
            let directive = ExecutionDirective::execute_attempt(
                command.campaign_id(),
                *rollout_id,
                progress.binding().request_digest(),
                attempt,
                None,
            )?;
            Ok(vec![outbox(
                directive.outbox_id()?,
                EXECUTION_DESTINATION,
                directive.canonical_bytes()?,
            )?])
        }
        EvaluationCommandKind::RetainRetryableAttemptAndRetry { rollout_id, retry } => {
            let progress = state
                .rollout(*rollout_id)
                .ok_or_else(|| binding::binding("retained retry rollout vanished"))?;
            if progress.status() != (RolloutStatus::RetryPending { retry: *retry }) {
                return Err(binding::binding("retained retry state differs from its command"));
            }
            let directive = ExecutionDirective::execute_attempt(
                command.campaign_id(),
                *rollout_id,
                progress.binding().request_digest(),
                retry.next_attempt(),
                Some(*retry),
            )?;
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
        EvaluationCommandKind::CreateCampaign { dataset_artifact, profile_artifact, .. }
        | EvaluationCommandKind::CreateCampaignWithStatePage {
            dataset_artifact,
            profile_artifact,
            ..
        } => vec![
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
        EvaluationCommandKind::RetainRetryableAttemptAndRetry { retry, .. } => {
            vec![ArtifactDependency::new(retry.retained().artifact().sha256())]
        }
        EvaluationCommandKind::StartRetryRollout { retry, .. } => {
            vec![ArtifactDependency::new(retry.retained().artifact().sha256())]
        }
        EvaluationCommandKind::CompleteAnalysis { artifact, .. } => {
            vec![ArtifactDependency::new(artifact.sha256())]
        }
        EvaluationCommandKind::CompleteReport { report } => {
            vec![ArtifactDependency::new(report.artifact().sha256())]
        }
        EvaluationCommandKind::RecordAnalysisSafePoint { safe_point }
        | EvaluationCommandKind::SettleAnalysisCancellation { safe_point } => {
            vec![ArtifactDependency::new(safe_point.artifact().sha256())]
        }
        EvaluationCommandKind::SuspendCampaign {
            analysis_safe_point: Some(safe_point),
            ..
        } => vec![ArtifactDependency::new(safe_point.artifact().sha256())],
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
            | EvaluationCommandKind::StartRetryRollout { .. }
            | EvaluationCommandKind::RetainRetryableAttempt { .. }
            | EvaluationCommandKind::RetainRetryableAttemptAndRetry { .. }
            | EvaluationCommandKind::SettleRollout { .. }
            | EvaluationCommandKind::SettleCancellation { .. }
            | EvaluationCommandKind::RecordPublication { .. }
            | EvaluationCommandKind::SettlePublicationCancellation { .. } => {
                Err(binding::binding("effect transition requires its exact claimed directive"))
            }
            _ => Ok(()),
        },
        CommitMode::Claimed(claim) => match (command.kind(), claim) {
            (
                EvaluationCommandKind::StartRollout { rollout_id, attempt, .. },
                EvaluationDirectiveClaim::Execution(value),
            ) if value.directive().campaign_id() == command.campaign_id()
                && value.directive().rollout_id() == *rollout_id
                && execution_claim_matches(state, *rollout_id, *attempt, value, None)
                && matches!(
                    state.rollout(*rollout_id).map(crate::RolloutProgress::status),
                    Some(RolloutStatus::Running { attempt: running }) if running == *attempt
                ) =>
            {
                Ok(())
            }
            (
                EvaluationCommandKind::StartRetryRollout { rollout_id, retry, .. },
                EvaluationDirectiveClaim::Execution(value),
            ) if value.directive().campaign_id() == command.campaign_id()
                && value.directive().rollout_id() == *rollout_id
                && execution_claim_matches(
                    state,
                    *rollout_id,
                    retry.next_attempt(),
                    value,
                    Some(*retry),
                )
                && state.rollout(*rollout_id).is_some_and(|progress| {
                    progress.status()
                        == (RolloutStatus::RetryRunning { retry: *retry })
                }) =>
            {
                Ok(())
            }
            _ => Err(binding::binding("claimed directive differs from pre-effect transition")),
        },
        CommitMode::Settlement(claim) => validate_settlement(command, state, claim),
    }
}

fn validate_settlement(
    command: &EvaluationCommand,
    state: &EvaluationState,
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
                && execution_settlement_matches(command, state, *rollout_id, value)
        }
        (
            EvaluationCommandKind::RetainRetryableAttemptAndRetry { rollout_id, retry },
            EvaluationDirectiveClaim::Execution(value),
        ) => {
            value.directive().campaign_id() == command.campaign_id()
                && value.directive().rollout_id() == *rollout_id
                && matches!(
                    value.directive().kind(),
                    ExecutionDirectiveKind::ExecuteAttempt {
                        attempt,
                        ..
                    } if attempt == retry.retained().attempt()
                )
                && execution_claim_matches(
                    state,
                    *rollout_id,
                    retry.retained().attempt(),
                    value,
                    None,
                )
        }
        (
            EvaluationCommandKind::RecordPublication { publication },
            EvaluationDirectiveClaim::Publication(value),
        ) => {
            value.directive().campaign_id() == command.campaign_id()
                && value.directive().report().id() == publication.report_id()
        }
        (
            EvaluationCommandKind::SettlePublicationCancellation { cancellation },
            EvaluationDirectiveClaim::Publication(value),
        ) => {
            value.directive().campaign_id() == command.campaign_id()
                && value.directive().report() == cancellation.report()
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
                    ExecutionDirectiveKind::Execute { .. }
                        | ExecutionDirectiveKind::ExecuteAttempt { .. }
                        | ExecutionDirectiveKind::Cancel
                )
        }
        _ => false,
    };
    if matches { Ok(()) } else { Err(binding::binding("claim differs from effect settlement")) }
}

fn execution_settlement_matches(
    command: &EvaluationCommand,
    state: &EvaluationState,
    rollout_id: crate::RolloutId,
    claim: &crate::ExecutionDirectiveClaim,
) -> bool {
    let expected_attempt = match command.kind() {
        EvaluationCommandKind::RetainRetryableAttempt { attempt, .. } => *attempt,
        EvaluationCommandKind::SettleRollout { terminal, .. } => terminal.attempt(),
        _ => return false,
    };
    execution_claim_matches(state, rollout_id, expected_attempt, claim, None)
}

fn execution_claim_matches(
    state: &EvaluationState,
    rollout_id: crate::RolloutId,
    expected_attempt: u16,
    claim: &crate::ExecutionDirectiveClaim,
    expected_retry: Option<crate::RetryIntent>,
) -> bool {
    let Some(progress) = state.rollout(rollout_id) else {
        return false;
    };
    match claim.directive().kind() {
        ExecutionDirectiveKind::Execute { request_digest } => {
            expected_retry.is_none() && request_digest == progress.binding().request_digest()
        }
        ExecutionDirectiveKind::ExecuteAttempt { request_digest, attempt, retry } => {
            request_digest == progress.binding().request_digest()
                && attempt == expected_attempt
                && expected_retry.is_none_or(|expected| retry == Some(expected))
        }
        ExecutionDirectiveKind::Cancel => false,
    }
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
    event: &EvaluationEvent,
    event_bytes: &[u8],
    state: &EvaluationState,
    request_digest: peritus_types::Sha256Digest,
    expected_receipt: EvaluationOperationReceipt,
    mode: &CommitMode,
) -> Result<Option<CommittedEvaluationOperation>, EvaluationError> {
    let exact_request = match journal
        .resolve_command(command.command_id(), request_digest)
        .map_err(journal_error)?
    {
        CommandResolution::Committed(_) => true,
        CommandResolution::Conflict { .. } => {
            if !mode.is_claim_bound() {
                return Err(binding::binding(
                    "command identity was committed with another exact request",
                ));
            }
            false
        }
        CommandResolution::DefinitelyAbsent => return Ok(None),
    };
    let operation =
        load_evaluation_operation(journal, command.campaign_id(), command.command_id())?
            .ok_or_else(|| recovery("resolved evaluation command has no retained operation"))?;
    if operation.batch().records().len() != 1
        || operation.batch().records()[0].frame_bytes() != event_bytes
        || operation.batch().records()[0].aggregate() != aggregate
        || operation.event() != event
        || operation.historical_state() != state
    {
        return Err(binding::binding(
            "resolved command identity belongs to another evaluation transition",
        ));
    }
    validate_expected_receipt(operation.receipt(), expected_receipt, mode, exact_request)?;
    Ok(Some(operation))
}

/// Loads one exact accepted evaluation operation independently of the current aggregate frontier.
///
/// Historical event/state are reconstructed from immutable records and checked against the exact
/// historical checkpoint root. Current state is rebuilt separately from the complete aggregate.
///
/// # Errors
/// Returns a typed integrity failure for detached command evidence, malformed history, an invalid
/// retained claim receipt, or divergent current state.
pub fn load_evaluation_operation(
    journal: &SqliteJournal,
    campaign_id: crate::EvaluationCampaignId,
    command_id: CommandId,
) -> Result<Option<CommittedEvaluationOperation>, EvaluationError> {
    let Some(batch) = journal.command_batch(command_id).map_err(journal_error)? else {
        return Ok(None);
    };
    operation_from_batch(journal, campaign_id, batch).map(Some)
}

fn operation_from_batch(
    journal: &SqliteJournal,
    campaign_id: crate::EvaluationCampaignId,
    batch: CommittedBatch,
) -> Result<CommittedEvaluationOperation, EvaluationError> {
    let aggregate = evaluation_aggregate_key(campaign_id)?;
    let [record] = batch.records() else {
        return Err(recovery(
            "evaluation operation command batch is not exactly one event",
        ));
    };
    if record.aggregate() != aggregate || record.command_id() != batch.command_id() {
        return Err(recovery(
            "evaluation operation event belongs to another aggregate or command",
        ));
    }
    let sequence = record.sequence().get();
    let event_index = usize::try_from(
        sequence
            .checked_sub(1)
            .ok_or_else(|| recovery("evaluation operation sequence is zero"))?,
    )
    .map_err(|_| recovery("evaluation operation sequence overflows memory indexing"))?;
    let replay_observation = load_evaluation_replay(journal, campaign_id)?;
    let event = replay_observation
        .events()
        .get(event_index)
        .cloned()
        .ok_or_else(|| recovery("evaluation operation event is absent from aggregate replay"))?;
    let historical_state = crate::replay(&replay_observation.events()[..=event_index])?;
    let current_state = replay_observation
        .rebuild()?
        .ok_or_else(|| recovery("accepted evaluation operation has no current aggregate"))?;
    let state_key = evaluation_state_key(campaign_id);
    let stored_historical = journal
        .state_record_revision(EVALUATION_STATE_NAMESPACE, &state_key, sequence)
        .map_err(journal_error)?
        .ok_or_else(|| recovery("evaluation operation has no historical successor checkpoint"))?;
    super::checkpoint::validate_historical(
        &stored_historical,
        campaign_id,
        &historical_state,
    )?;
    if event.campaign_id() != campaign_id
        || event.sequence() != sequence
        || event.id() != record.event_id()
        || event.command_id() != record.command_id()
        || event.previous_event() != record.previous_event_id()
        || peritus_evidence::revision_digest(historical_state.revision())
            != record.revision_digest()
        || stored_historical.producing_position() != batch.last_position()
        || stored_historical.revision() != sequence
    {
        return Err(recovery(
            "evaluation operation event and historical successor checkpoint differ",
        ));
    }
    let receipt_record = journal
        .state_record(EVALUATION_RECEIPT_NAMESPACE, &receipt_key(batch.command_id()))
        .map_err(journal_error)?;
    let receipt = match receipt_record {
        Some(record) => {
            if record.revision() != 1 || record.producing_position() != batch.last_position() {
                return Err(recovery(
                    "evaluation operation receipt was not installed with its command batch",
                ));
            }
            let receipt = EvaluationOperationReceipt::decode(record.bytes())?;
            validate_retained_receipt(
                &receipt,
                &batch,
                record.digest(),
                &event,
                &historical_state,
            )?;
            receipt
        }
        None => EvaluationOperationReceipt::legacy(
            batch.command_id(),
            event.id(),
            campaign_id,
            sequence,
            event.command_digest(),
            batch.request_digest(),
            record.frame_digest(),
            historical_state.state_digest(),
        ),
    };
    if current_state.sequence() < historical_state.sequence()
        || (current_state.sequence() == historical_state.sequence()
            && current_state != historical_state)
    {
        return Err(recovery(
            "current evaluation state is behind or differs from the historical operation",
        ));
    }
    Ok(CommittedEvaluationOperation::new(
        batch,
        receipt,
        event,
        historical_state,
        current_state,
    ))
}

fn validate_retained_receipt(
    receipt: &EvaluationOperationReceipt,
    batch: &CommittedBatch,
    stored_digest: peritus_types::Sha256Digest,
    event: &EvaluationEvent,
    state: &EvaluationState,
) -> Result<(), EvaluationError> {
    if peritus_codec::sha256(&receipt.canonical_bytes()?) != stored_digest
        || receipt.command_id() != batch.command_id()
        || receipt.event_id() != event.id()
        || receipt.campaign_id() != event.campaign_id()
        || receipt.sequence() != event.sequence()
        || receipt.command_digest() != event.command_digest()
        || receipt.request_digest() != batch.request_digest()
        || receipt.event_frame_digest() != batch.records()[0].frame_digest()
        || receipt.successor_state_digest() != state.state_digest()
    {
        return Err(recovery(
            "retained evaluation operation receipt differs from immutable history",
        ));
    }
    Ok(())
}

fn validate_expected_receipt(
    observed: EvaluationOperationReceipt,
    expected: EvaluationOperationReceipt,
    mode: &CommitMode,
    exact_request: bool,
) -> Result<(), EvaluationError> {
    if observed.mode() == EvaluationCommitMode::Legacy {
        return if exact_request || mode.is_claim_bound() {
            Ok(())
        } else {
            Err(binding::binding(
                "legacy evaluation operation request digest differs",
            ))
        };
    }
    if observed.command_id() != expected.command_id()
        || observed.event_id() != expected.event_id()
        || observed.campaign_id() != expected.campaign_id()
        || observed.sequence() != expected.sequence()
        || observed.command_digest() != expected.command_digest()
        || observed.base_request_digest() != expected.base_request_digest()
        || observed.event_frame_digest() != expected.event_frame_digest()
        || observed.successor_state_digest() != expected.successor_state_digest()
        || observed.mode() != mode.receipt_mode()
    {
        return Err(binding::binding(
            "resolved evaluation operation receipt differs from the retry",
        ));
    }
    if exact_request {
        if observed != expected {
            return Err(binding::binding(
                "exact evaluation request has a different retained receipt",
            ));
        }
        return Ok(());
    }
    let (Some(original), Some(replacement)) =
        (observed.original_claim(), expected.original_claim())
    else {
        return Err(binding::binding(
            "claim-bound evaluation retry has no retained claim identity",
        ));
    };
    if original.outbox_id() != replacement.outbox_id()
        || replacement.fence() <= original.fence()
        || observed.request_digest() == expected.request_digest()
    {
        return Err(binding::binding(
            "replacement evaluation claim does not advance the original retained fence",
        ));
    }
    Ok(())
}

fn operation_receipt(
    command: &EvaluationCommand,
    event: &EvaluationEvent,
    event_frame_digest: peritus_types::Sha256Digest,
    base_request_digest: peritus_types::Sha256Digest,
    request_digest: peritus_types::Sha256Digest,
    mode: &CommitMode,
) -> Result<EvaluationOperationReceipt, EvaluationError> {
    let original_claim = mode
        .claim()
        .map(|claim| {
            claim
                .outbox_id()
                .map(|id| claim_receipt(id, claim.fence()))
        })
        .transpose()?;
    EvaluationOperationReceipt::retained(
        command.command_id(),
        event.id(),
        command.campaign_id(),
        event.sequence(),
        command.digest(),
        base_request_digest,
        request_digest,
        event_frame_digest,
        event.successor_state_digest(),
        mode.receipt_mode(),
        original_claim,
    )
}

fn outbox(
    id: peritus_journal::OutboxId,
    destination: &str,
    payload: Vec<u8>,
) -> Result<OutboxDraft, EvaluationError> {
    OutboxDraft::persistent(id, destination.to_owned(), payload).map_err(journal_error)
}
fn bound_digest(
    domain: &[u8],
    base: peritus_types::Sha256Digest,
    claim: &EvaluationDirectiveClaim,
) -> Result<peritus_types::Sha256Digest, EvaluationError> {
    let mut bytes = Vec::with_capacity(domain.len() + 32 + 16 + 8);
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(base.as_bytes());
    bytes.extend_from_slice(claim.outbox_id()?.as_bytes());
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
