//! Pure evaluation command decision, event application, and replay.

use peritus_codec::CanonicalWriter;

use crate::{
    EvaluationCommand, EvaluationCommandKind, EvaluationError, EvaluationErrorKind,
    EvaluationEvent, EvaluationEventKind, EvaluationOperation, EvaluationPhase, EvaluationRecovery,
    EvaluationState, EvaluationTransition, RolloutProgress, RolloutStatus,
};

/// Decides one pure command against exact current state.
///
/// # Errors
/// Rejects stale fences, illegal phases, unknown rollouts, gaps, conflicting settlement, and
/// incomplete accounting.
pub fn decide(
    prior: Option<&EvaluationState>,
    command: &EvaluationCommand,
) -> Result<EvaluationTransition, EvaluationError> {
    validate_command_fence(prior, command)?;
    let sequence = command.expected_sequence().checked_add(1).ok_or_else(transition)?;
    let mut state = apply_kind(
        prior,
        command.campaign_id(),
        command.profile_digest(),
        sequence,
        command.event_id(),
        command.kind(),
    )?;
    state.refresh_digest()?;
    let event = EvaluationEvent::new(
        command.event_id(),
        command.command_id(),
        command.campaign_id(),
        sequence,
        command.expected_previous_event(),
        command.prior_state_digest(),
        command.profile_digest(),
        command.digest(),
        state.state_digest(),
        EvaluationEventKind::Accepted(command.kind().clone()),
    );
    Ok(EvaluationTransition::new(event, state))
}

/// Applies one checked event deterministically against its exact predecessor.
///
/// # Errors
/// Rejects gaps, broken bindings, illegal semantics, or successor-digest disagreement.
pub fn apply_event(
    prior: Option<&EvaluationState>,
    event: &EvaluationEvent,
) -> Result<EvaluationState, EvaluationError> {
    let expected_sequence = prior.map_or(1, |state| state.sequence().saturating_add(1));
    let expected_previous = prior.map(EvaluationState::last_event_id);
    let expected_prior_digest =
        prior.map_or(peritus_types::Sha256Digest::new([0; 32]), EvaluationState::state_digest);
    if event.sequence() != expected_sequence
        || event.previous_event() != expected_previous
        || event.prior_state_digest() != expected_prior_digest
        || prior.is_some_and(|state| {
            state.campaign_id() != event.campaign_id()
                || state.profile_digest() != event.profile_digest()
        })
    {
        return Err(binding("evaluation event predecessor or immutable binding differs"));
    }
    let EvaluationEventKind::Accepted(kind) = event.kind();
    let mut state = apply_kind(
        prior,
        event.campaign_id(),
        event.profile_digest(),
        event.sequence(),
        event.id(),
        kind,
    )?;
    state.refresh_digest()?;
    if state.state_digest() != event.successor_state_digest() {
        return Err(binding("evaluation event successor digest differs from pure application"));
    }
    Ok(state)
}

/// Folds a complete contiguous event history.
///
/// # Errors
/// Rejects an empty history or the first invalid event.
pub fn replay(events: &[EvaluationEvent]) -> Result<EvaluationState, EvaluationError> {
    let mut state = None;
    for event in events {
        state = Some(apply_event(state.as_ref(), event)?);
    }
    state.ok_or_else(|| binding("evaluation replay history is empty"))
}

fn validate_command_fence(
    prior: Option<&EvaluationState>,
    command: &EvaluationCommand,
) -> Result<(), EvaluationError> {
    match prior {
        None => {
            if command.expected_sequence() != 0
                || command.expected_previous_event().is_some()
                || command.prior_state_digest() != peritus_types::Sha256Digest::new([0; 32])
                || !matches!(command.kind(), EvaluationCommandKind::CreateCampaign { .. })
            {
                return Err(binding("campaign creation command has a non-genesis fence"));
            }
        }
        Some(state) => {
            if state.phase().terminal()
                || command.expected_sequence() != state.sequence()
                || command.expected_previous_event() != Some(state.last_event_id())
                || command.prior_state_digest() != state.state_digest()
                || command.campaign_id() != state.campaign_id()
                || command.profile_digest() != state.profile_digest()
                || matches!(command.kind(), EvaluationCommandKind::CreateCampaign { .. })
            {
                return Err(binding("evaluation command fence or immutable binding differs"));
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines, reason = "closed campaign transition table stays explicit")]
fn apply_kind(
    prior: Option<&EvaluationState>,
    campaign_id: crate::EvaluationCampaignId,
    profile_digest: crate::ProfileDigest,
    sequence: u64,
    event_id: peritus_types::EventId,
    kind: &EvaluationCommandKind,
) -> Result<EvaluationState, EvaluationError> {
    if let EvaluationCommandKind::CreateCampaign {
        revision,
        dataset_digest,
        dataset_artifact,
        profile_artifact,
    } = kind
    {
        if prior.is_some() || sequence != 1 {
            return Err(transition());
        }
        return Ok(EvaluationState {
            campaign_id,
            revision: *revision,
            dataset_digest: *dataset_digest,
            dataset_artifact: *dataset_artifact,
            profile_artifact: *profile_artifact,
            profile_digest,
            sequence,
            last_event_id: event_id,
            state_digest: peritus_types::Sha256Digest::new([0; 32]),
            phase: EvaluationPhase::Created,
            batch_total: None,
            pending_plan_id: None,
            pending_plan_digest: None,
            batch_artifacts: Vec::new(),
            rollouts: std::collections::BTreeMap::new(),
            plan: None,
            analysis_digest: None,
            analysis_artifact: None,
            analysis_artifact_bytes: None,
            analysis_counts: None,
            report: None,
            publication: None,
            cancellation_reason: None,
            failure: None,
        });
    }
    let mut state = prior.cloned().ok_or_else(transition)?;
    state.sequence = sequence;
    state.last_event_id = event_id;
    match kind {
        EvaluationCommandKind::CreateCampaign { .. } => return Err(transition()),
        EvaluationCommandKind::RecordPlanBatch { plan_id, plan_digest, batch } => {
            require_phase(&state, &[EvaluationPhase::Created])?;
            if state.plan.is_some()
                || batch.ordinal()
                    != u32::try_from(state.batch_artifacts.len())
                        .map_err(|_| transition())?
                        .checked_add(1)
                        .ok_or_else(transition)?
                || state.batch_total.is_some_and(|total| total != batch.total_batches())
                || state.pending_plan_id.is_some_and(|id| id != *plan_id)
                || state.pending_plan_digest.is_some_and(|digest| digest != *plan_digest)
            {
                return Err(binding("plan batch is gapped or differs from prior plan identity"));
            }
            state.batch_total = Some(batch.total_batches());
            state.pending_plan_id = Some(*plan_id);
            state.pending_plan_digest = Some(*plan_digest);
            for binding_value in batch.bindings() {
                if state
                    .rollouts
                    .insert(binding_value.rollout_id(), RolloutProgress::planned(*binding_value))
                    .is_some()
                {
                    return Err(binding("plan batch repeats a rollout identity"));
                }
            }
            state.batch_artifacts.push(batch.artifact());
        }
        EvaluationCommandKind::CompletePlan { plan } => {
            require_phase(&state, &[EvaluationPhase::Created])?;
            let batch_count =
                u32::try_from(state.batch_artifacts.len()).map_err(|_| transition())?;
            let rollout_count = u32::try_from(state.rollouts.len()).map_err(|_| transition())?;
            if state.batch_total != Some(batch_count)
                || batch_count != plan.total_batches()
                || rollout_count != plan.expected_rollouts()
                || state.pending_plan_id != Some(plan.id())
                || state.pending_plan_digest != Some(plan.digest())
            {
                return Err(binding(
                    "complete plan differs from recorded batch identity or counts",
                ));
            }
            state.plan = Some(*plan);
            state.phase = EvaluationPhase::Planned;
        }
        EvaluationCommandKind::RequestSchedule { rollout_id, work } => {
            require_phase(
                &state,
                &[EvaluationPhase::Planned, EvaluationPhase::Scheduling, EvaluationPhase::Running],
            )?;
            let progress = state.rollouts.get_mut(rollout_id).ok_or_else(unknown_rollout)?;
            if progress.status() != RolloutStatus::Planned
                || progress.binding().work_id() != work.id()
                || progress.binding().request_digest() != work.payload_digest()
                || work.class() != peritus_scheduler::ExecutionClass::Coordination
                || work.revision() != state.revision
            {
                return Err(binding("schedule request rollout is not in planned state"));
            }
            progress.set_status(RolloutStatus::Scheduling);
            if state.phase == EvaluationPhase::Planned {
                state.phase = EvaluationPhase::Scheduling;
            }
        }
        EvaluationCommandKind::RecordSchedule { rollout_id, acknowledgement_digest } => {
            require_phase(&state, &[EvaluationPhase::Scheduling, EvaluationPhase::Running])?;
            let progress = state.rollouts.get_mut(rollout_id).ok_or_else(unknown_rollout)?;
            if progress.status() != RolloutStatus::Scheduling {
                return Err(binding("schedule acknowledgement has no outstanding request"));
            }
            progress.set_status(RolloutStatus::Scheduled {
                acknowledgement_digest: *acknowledgement_digest,
            });
        }
        EvaluationCommandKind::StartRollout { rollout_id, attempt, .. } => {
            require_phase(&state, &[EvaluationPhase::Scheduling, EvaluationPhase::Running])?;
            let progress = state.rollouts.get_mut(rollout_id).ok_or_else(unknown_rollout)?;
            if !matches!(progress.status(), RolloutStatus::Scheduled { .. })
                || progress.attempts_retained().checked_add(1) != Some(*attempt)
            {
                return Err(binding("rollout start is not the next scheduled attempt"));
            }
            progress.set_status(RolloutStatus::Running { attempt: *attempt });
            state.phase = EvaluationPhase::Running;
        }
        EvaluationCommandKind::RetainRetryableAttempt { rollout_id, attempt, .. } => {
            require_phase(&state, &[EvaluationPhase::Running])?;
            let progress = state.rollouts.get_mut(rollout_id).ok_or_else(unknown_rollout)?;
            if progress.status() != (RolloutStatus::Running { attempt: *attempt }) {
                return Err(binding("retryable attempt differs from the running attempt"));
            }
            progress.retain_attempt(*attempt);
            progress.set_status(RolloutStatus::Scheduled {
                acknowledgement_digest: progress.binding().request_digest(),
            });
        }
        EvaluationCommandKind::SettleRollout { rollout_id, terminal } => {
            require_phase(&state, &[EvaluationPhase::Running])?;
            let progress = state.rollouts.get_mut(rollout_id).ok_or_else(unknown_rollout)?;
            if progress.status() != (RolloutStatus::Running { attempt: terminal.attempt() })
                || progress.attempts_retained().checked_add(1) != Some(terminal.attempt())
            {
                return Err(binding("terminal result differs from the running attempt"));
            }
            progress.retain_attempt(terminal.attempt());
            progress.set_status(RolloutStatus::Settled(*terminal));
        }
        EvaluationCommandKind::CancelCampaign { reason_digest } => {
            if state.phase == EvaluationPhase::Analyzing
                || state.phase == EvaluationPhase::ReportReady
            {
                return Err(binding("campaign cancellation is too late after analysis began"));
            }
            state.phase = EvaluationPhase::Cancelling;
            state.cancellation_reason = Some(*reason_digest);
        }
        EvaluationCommandKind::SettleCancellation { rollout_id, observation_digest } => {
            require_phase(&state, &[EvaluationPhase::Cancelling])?;
            let reason = state.cancellation_reason.ok_or_else(transition)?;
            let progress = state.rollouts.get_mut(rollout_id).ok_or_else(unknown_rollout)?;
            if !matches!(
                progress.status(),
                RolloutStatus::Scheduling
                    | RolloutStatus::Scheduled { .. }
                    | RolloutStatus::Running { .. }
            ) {
                return Err(binding("cancellation settlement has no outstanding external work"));
            }
            progress.set_status(RolloutStatus::Cancelled {
                reason_digest: reason,
                observation_digest: *observation_digest,
            });
        }
        EvaluationCommandKind::CompleteCancellation => {
            require_phase(&state, &[EvaluationPhase::Cancelling])?;
            let reason = state.cancellation_reason.ok_or_else(transition)?;
            for progress in state.rollouts.values_mut() {
                match progress.status() {
                    RolloutStatus::Planned => {
                        progress.set_status(RolloutStatus::Cancelled {
                            reason_digest: reason,
                            observation_digest: reason,
                        });
                    }
                    RolloutStatus::Settled(_) | RolloutStatus::Cancelled { .. } => {}
                    RolloutStatus::Scheduling
                    | RolloutStatus::Scheduled { .. }
                    | RolloutStatus::Running { .. } => {
                        return Err(binding(
                            "campaign cancellation still has outstanding external work",
                        ));
                    }
                }
            }
            if !state.counts().complete() {
                return Err(transition());
            }
            state.phase = EvaluationPhase::Cancelled;
        }
        EvaluationCommandKind::StartAnalysis { counts } => {
            require_phase(
                &state,
                &[EvaluationPhase::Planned, EvaluationPhase::Scheduling, EvaluationPhase::Running],
            )?;
            let observed = state.counts();
            if !observed.complete() || observed != *counts {
                return Err(binding("analysis counts differ from complete rollout conservation"));
            }
            state.analysis_counts = Some(*counts);
            state.phase = EvaluationPhase::Analyzing;
        }
        EvaluationCommandKind::CompleteAnalysis { analysis_digest, artifact, artifact_bytes } => {
            require_phase(&state, &[EvaluationPhase::Analyzing])?;
            if state.analysis_digest.is_some() || *artifact_bytes == 0 {
                return Err(binding("analysis result already exists or has zero bytes"));
            }
            state.analysis_digest = Some(*analysis_digest);
            state.analysis_artifact = Some(*artifact);
            state.analysis_artifact_bytes = Some(*artifact_bytes);
        }
        EvaluationCommandKind::CompleteReport { report } => {
            require_phase(&state, &[EvaluationPhase::Analyzing])?;
            if state.analysis_digest.is_none() || state.report.is_some() {
                return Err(binding("report completion requires exactly one completed analysis"));
            }
            state.report = Some(*report);
            state.phase = EvaluationPhase::ReportReady;
        }
        EvaluationCommandKind::RecordPublication { publication } => {
            require_phase(&state, &[EvaluationPhase::ReportReady])?;
            if state.report.map(crate::ReportRecord::id) != Some(publication.report_id()) {
                return Err(binding("publication differs from the committed report"));
            }
            state.publication = Some(*publication);
            state.phase = EvaluationPhase::Published;
        }
        EvaluationCommandKind::FailCampaign { failure } => {
            state.failure = Some(*failure);
            state.phase = EvaluationPhase::Failed;
        }
    }
    Ok(state)
}

fn require_phase(
    state: &EvaluationState,
    allowed: &[EvaluationPhase],
) -> Result<(), EvaluationError> {
    if allowed.contains(&state.phase()) { Ok(()) } else { Err(transition()) }
}
const fn unknown_rollout() -> EvaluationError {
    binding("command references an unknown rollout")
}
const fn transition() -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::IllegalTransition,
        EvaluationOperation::ApplyTransition,
        EvaluationRecovery::Replay,
        "evaluation command is illegal in the current campaign phase",
    )
}
const fn binding(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Binding,
        EvaluationOperation::ApplyTransition,
        EvaluationRecovery::Quarantine,
        detail,
    )
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "canonical durability encoding is also consumed by a sibling module"
)]
pub(crate) fn encode_kind(
    writer: &mut CanonicalWriter,
    kind: &EvaluationCommandKind,
) -> Result<(), EvaluationError> {
    match kind {
        EvaluationCommandKind::CreateCampaign {
            revision,
            dataset_digest,
            dataset_artifact,
            profile_artifact,
        } => {
            writer.write_u8(1).map_err(codec)?;
            encode_revision(writer, *revision)?;
            writer.write_fixed(dataset_digest.as_bytes()).map_err(codec)?;
            writer.write_fixed(dataset_artifact.as_bytes()).map_err(codec)?;
            writer.write_fixed(profile_artifact.as_bytes()).map_err(codec)?;
        }
        EvaluationCommandKind::RecordPlanBatch { plan_id, plan_digest, batch } => {
            writer.write_u8(2).map_err(codec)?;
            writer.write_fixed(plan_id.as_bytes()).map_err(codec)?;
            writer.write_fixed(plan_digest.as_bytes()).map_err(codec)?;
            writer.write_u32(batch.ordinal()).map_err(codec)?;
            writer.write_u32(batch.total_batches()).map_err(codec)?;
            writer.write_fixed(batch.artifact().as_bytes()).map_err(codec)?;
            writer.write_collection_len(batch.bindings().len()).map_err(codec)?;
            for binding in batch.bindings() {
                writer.write_fixed(binding.rollout_id().as_bytes()).map_err(codec)?;
                writer.write_fixed(binding.work_id().as_bytes()).map_err(codec)?;
                writer.write_fixed(binding.request_digest().as_bytes()).map_err(codec)?;
            }
        }
        EvaluationCommandKind::CompletePlan { plan } => {
            writer.write_u8(3).map_err(codec)?;
            encode_plan(writer, *plan)?;
        }
        EvaluationCommandKind::RequestSchedule { rollout_id, work } => {
            writer.write_u8(4).map_err(codec)?;
            writer.write_fixed(rollout_id.as_bytes()).map_err(codec)?;
            encode_work(writer, work)?;
        }
        EvaluationCommandKind::RecordSchedule { rollout_id, acknowledgement_digest } => {
            writer.write_u8(5).map_err(codec)?;
            writer.write_fixed(rollout_id.as_bytes()).map_err(codec)?;
            writer.write_fixed(acknowledgement_digest.as_bytes()).map_err(codec)?;
        }
        EvaluationCommandKind::StartRollout { rollout_id, attempt, started_at_tick } => {
            writer.write_u8(6).map_err(codec)?;
            writer.write_fixed(rollout_id.as_bytes()).map_err(codec)?;
            writer.write_u16(*attempt).map_err(codec)?;
            writer.write_u64(*started_at_tick).map_err(codec)?;
        }
        EvaluationCommandKind::RetainRetryableAttempt {
            rollout_id,
            attempt,
            observation_digest,
        } => {
            writer.write_u8(7).map_err(codec)?;
            writer.write_fixed(rollout_id.as_bytes()).map_err(codec)?;
            writer.write_u16(*attempt).map_err(codec)?;
            writer.write_fixed(observation_digest.as_bytes()).map_err(codec)?;
        }
        EvaluationCommandKind::SettleRollout { rollout_id, terminal } => {
            writer.write_u8(8).map_err(codec)?;
            writer.write_fixed(rollout_id.as_bytes()).map_err(codec)?;
            encode_terminal(writer, *terminal)?;
        }
        EvaluationCommandKind::CancelCampaign { reason_digest } => {
            writer.write_u8(9).map_err(codec)?;
            writer.write_fixed(reason_digest.as_bytes()).map_err(codec)?;
        }
        EvaluationCommandKind::SettleCancellation { rollout_id, observation_digest } => {
            writer.write_u8(16).map_err(codec)?;
            writer.write_fixed(rollout_id.as_bytes()).map_err(codec)?;
            writer.write_fixed(observation_digest.as_bytes()).map_err(codec)?;
        }
        EvaluationCommandKind::CompleteCancellation => writer.write_u8(10).map_err(codec)?,
        EvaluationCommandKind::StartAnalysis { counts } => {
            writer.write_u8(11).map_err(codec)?;
            encode_counts(writer, *counts)?;
        }
        EvaluationCommandKind::CompleteAnalysis { analysis_digest, artifact, artifact_bytes } => {
            writer.write_u8(12).map_err(codec)?;
            writer.write_fixed(analysis_digest.as_bytes()).map_err(codec)?;
            writer.write_fixed(artifact.as_bytes()).map_err(codec)?;
            writer.write_u64(*artifact_bytes).map_err(codec)?;
        }
        EvaluationCommandKind::CompleteReport { report } => {
            writer.write_u8(13).map_err(codec)?;
            encode_report(writer, *report)?;
        }
        EvaluationCommandKind::RecordPublication { publication } => {
            writer.write_u8(14).map_err(codec)?;
            writer.write_fixed(publication.report_id().as_bytes()).map_err(codec)?;
            writer.write_fixed(publication.evidence_id().as_bytes()).map_err(codec)?;
            writer.write_u64(publication.report_commit_position()).map_err(codec)?;
        }
        EvaluationCommandKind::FailCampaign { failure } => {
            writer.write_u8(15).map_err(codec)?;
            writer.write_u8(failure.code().tag()).map_err(codec)?;
            writer.write_fixed(failure.digest().as_bytes()).map_err(codec)?;
        }
    }
    Ok(())
}

fn encode_revision(
    writer: &mut CanonicalWriter,
    value: peritus_types::RevisionTuple,
) -> Result<(), EvaluationError> {
    writer.write_fixed(value.acceptance_spec_id().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.harness_id().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.workspace_id().as_bytes()).map_err(codec)?;
    writer.write_u64(value.workspace_generation().get()).map_err(codec)?;
    writer.write_u64(value.workspace_revision().get()).map_err(codec)?;
    writer.write_fixed(value.policy_id().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.provider_profile_id().as_bytes()).map_err(codec)
}
#[allow(
    clippy::redundant_pub_crate,
    reason = "canonical work encoding is also consumed by a sibling module"
)]
pub(crate) fn encode_work(
    writer: &mut CanonicalWriter,
    value: &peritus_scheduler::WorkSpec,
) -> Result<(), EvaluationError> {
    writer.write_fixed(value.id().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.owner().as_bytes()).map_err(codec)?;
    encode_revision(writer, value.revision())?;
    writer
        .write_u8(match value.class() {
            peritus_scheduler::ExecutionClass::Model => 1,
            peritus_scheduler::ExecutionClass::Tool => 2,
            peritus_scheduler::ExecutionClass::Gate => 3,
            peritus_scheduler::ExecutionClass::Review => 4,
            peritus_scheduler::ExecutionClass::Coordination => 5,
        })
        .map_err(codec)?;
    writer.write_u8(value.priority()).map_err(codec)?;
    writer.write_collection_len(value.request().entries().len()).map_err(codec)?;
    for entry in value.request().entries() {
        writer.write_u16(entry.kind().tag()).map_err(codec)?;
        writer.write_u64(entry.quantity().get()).map_err(codec)?;
    }
    writer.write_option_tag(value.budget_reservation().is_some()).map_err(codec)?;
    if let Some(id) = value.budget_reservation() {
        writer.write_fixed(id.as_bytes()).map_err(codec)?;
    }
    writer.write_collection_len(value.dependencies().len()).map_err(codec)?;
    for dependency in value.dependencies() {
        writer.write_fixed(dependency.as_bytes()).map_err(codec)?;
    }
    writer.write_option_tag(value.parent().is_some()).map_err(codec)?;
    if let Some(parent) = value.parent() {
        writer.write_fixed(parent.as_bytes()).map_err(codec)?;
    }
    writer.write_u16(value.maximum_attempts().get()).map_err(codec)?;
    writer
        .write_u8(match value.recovery() {
            peritus_scheduler::RecoveryPolicy::RetrySafe => 1,
            peritus_scheduler::RecoveryPolicy::Ambiguous => 2,
            peritus_scheduler::RecoveryPolicy::Fail => 3,
        })
        .map_err(codec)?;
    writer.write_fixed(value.payload_digest().as_bytes()).map_err(codec)
}
fn encode_plan(
    writer: &mut CanonicalWriter,
    value: crate::PlanRecord,
) -> Result<(), EvaluationError> {
    writer.write_fixed(value.id().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.digest().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.root().as_bytes()).map_err(codec)?;
    writer.write_u32(value.expected_rollouts()).map_err(codec)?;
    writer.write_u32(value.total_batches()).map_err(codec)
}
fn encode_terminal(
    writer: &mut CanonicalWriter,
    value: crate::TerminalRecordRef,
) -> Result<(), EvaluationError> {
    writer.write_u8(value.class().tag()).map_err(codec)?;
    writer.write_fixed(value.record_digest().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.artifact().as_bytes()).map_err(codec)?;
    writer.write_u64(value.artifact_bytes()).map_err(codec)?;
    writer.write_u16(value.attempt()).map_err(codec)
}
fn encode_counts(
    writer: &mut CanonicalWriter,
    value: crate::LedgerCounts,
) -> Result<(), EvaluationError> {
    for item in [
        value.expected,
        value.passed,
        value.task_failed,
        value.infrastructure_failed,
        value.cancelled,
        value.ambiguous,
    ] {
        writer.write_u32(item).map_err(codec)?;
    }
    Ok(())
}
fn encode_report(
    writer: &mut CanonicalWriter,
    value: crate::ReportRecord,
) -> Result<(), EvaluationError> {
    writer.write_fixed(value.id().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.payload_digest().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.artifact().as_bytes()).map_err(codec)?;
    writer.write_u64(value.size()).map_err(codec)
}
const fn codec(_: peritus_codec::CodecError) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::LimitExceeded,
        EvaluationOperation::Codec,
        EvaluationRecovery::ReduceScope,
        "evaluation semantic payload exceeds canonical codec limits",
    )
}
