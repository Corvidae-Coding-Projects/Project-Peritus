//! Pure deterministic D2 transition, terminal truth, and replay.

mod apply;

use std::collections::BTreeSet;

use peritus_types::{EventSequence, Sha256Digest};

use crate::error::{ReviewError, ReviewErrorKind, reject};
use crate::state::mutation;
use crate::{
    ReviewCommand, ReviewCommandKind, ReviewEvent, ReviewEventKind, ReviewRunPhase, ReviewRunState,
    ReviewTransition,
};

use apply::apply;

/// Starts a review run from the only legal genesis command.
///
/// # Errors
/// Rejects a non-genesis fence, run/revision mismatch, invalid binding/limits, or non-start command.
pub fn start(command: &ReviewCommand) -> Result<ReviewTransition, ReviewError> {
    let ReviewCommandKind::StartRun { binding, limits } = command.kind() else {
        return Err(illegal("genesis command is not StartRun"));
    };
    binding.validate(*limits)?;
    if estimated_payload_bytes(command.kind()) > limits.payload_bytes() {
        return Err(reject(
            ReviewErrorKind::LimitExceeded,
            "genesis command exceeds its immutable payload-byte limit",
        ));
    }
    if command.revision() != binding.revision()
        || command.expected_sequence() != 0
        || command.expected_previous_event().is_some()
        || command.prior_state_digest() != Sha256Digest::new([0; 32])
    {
        return Err(reject(
            ReviewErrorKind::BindingMismatch,
            "genesis command differs from its exact binding or genesis fences",
        ));
    }
    let sequence = EventSequence::first();
    let mut state = ReviewRunState::genesis(
        command.run_id(),
        *limits,
        binding.clone(),
        sequence,
        command.event_id(),
        command.command_id(),
    );
    if state.estimated_encoded_bytes() > limits.state_bytes() {
        return Err(reject(
            ReviewErrorKind::LimitExceeded,
            "genesis state exceeds its immutable state-byte limit",
        ));
    }
    let successor = crate::canonical::state_digest(&state);
    mutation::set_state_digest(&mut state, successor);
    let event = ReviewEvent::from_wire(
        command.event_id(),
        command.command_id(),
        sequence,
        None,
        command.run_id(),
        command.revision(),
        Sha256Digest::new([0; 32]),
        successor,
        ReviewEventKind::RunStarted { binding: binding.clone(), limits: *limits },
    );
    Ok(ReviewTransition::new(event, state))
}

/// Applies one fenced command to cloned state without performing effects.
///
/// # Errors
/// Rejects stale fences, closed state, reused identities, malformed structured records, illegal
/// lifecycle transitions, unretained provenance, false authority, or bounded-state exhaustion.
pub fn decide(
    state: &ReviewRunState,
    command: &ReviewCommand,
) -> Result<ReviewTransition, ReviewError> {
    let mut hydration = crate::history::ReviewHistoryHydration::for_command(command);
    hydration.capture_active(state);
    decide_hydrated(state, command, hydration)
}

pub(crate) fn decide_hydrated(
    state: &ReviewRunState,
    command: &ReviewCommand,
    hydration: crate::history::ReviewHistoryHydration,
) -> Result<ReviewTransition, ReviewError> {
    validate_fences(state, command)?;
    if estimated_payload_bytes(command.kind()) > state.limits().payload_bytes() {
        return Err(reject(
            ReviewErrorKind::LimitExceeded,
            "review command exceeds its immutable payload-byte limit",
        ));
    }
    let sequence = state
        .sequence()
        .checked_next()
        .map_err(|_| reject(ReviewErrorKind::LimitExceeded, "review event sequence overflowed"))?;
    let transient_reviewer_cycle = match command.kind() {
        ReviewCommandKind::ConfirmResolution { reviewer_cycle, .. }
        | ReviewCommandKind::ConfirmInvalidation { reviewer_cycle, .. }
        | ReviewCommandKind::ConfirmSupersession { reviewer_cycle, .. }
            if state.cycle(*reviewer_cycle).is_none() => Some(*reviewer_cycle),
        _ => None,
    };
    let mut successor = state.clone();
    let advance_revision = matches!(command.kind(), ReviewCommandKind::AdvanceRevision { .. });
    mutation::archive_predecessor(
        &mut successor,
        advance_revision,
        advance_revision || should_archive(state, command),
    )?;
    hydration.install(&mut successor)?;
    let kind = apply(&mut successor, command.event_id(), command.kind())?;
    if let Some(cycle_id) = transient_reviewer_cycle {
        mutation::remove_cycle(&mut successor, cycle_id);
    }
    mutation::recompute(&mut successor);
    mutation::compact_active_dispositions(&mut successor);
    if successor.estimated_encoded_bytes() > successor.limits().state_bytes() {
        return Err(reject(
            ReviewErrorKind::LimitExceeded,
            "successor state exceeds its immutable state-byte limit",
        ));
    }
    mutation::advance_cursor(&mut successor, sequence, command.event_id(), command.command_id());
    let successor_digest = crate::canonical::state_digest(&successor);
    mutation::set_state_digest(&mut successor, successor_digest);
    let event = ReviewEvent::from_wire(
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
    Ok(ReviewTransition::new(event, successor))
}

/// Reconstructs exact state from genesis and canonical events.
///
/// # Errors
/// Rejects empty, duplicated, reordered, stale, tampered, or semantically illegal streams.
pub fn replay(events: &[ReviewEvent]) -> Result<ReviewRunState, ReviewError> {
    let first = events
        .first()
        .ok_or_else(|| reject(ReviewErrorKind::ReplayMismatch, "review replay is empty"))?;
    let first_command = command_from_event(first, 0, None)?;
    let first_transition = start(&first_command)?;
    if first_transition.event() != first {
        return Err(replay_error("genesis review event differs from deterministic reduction"));
    }
    let mut state = first_transition.into_state();
    let mut event_ids = BTreeSet::from([first.id()]);
    let mut command_ids = BTreeSet::from([first.command_id()]);
    for (index, event) in events[1..].iter().enumerate() {
        if !event_ids.insert(event.id()) || !command_ids.insert(event.command_id()) {
            return Err(replay_error("review event or command identity is duplicated"));
        }
        let command =
            command_from_event(event, state.sequence().get(), Some(state.last_event_id()))?;
        let mut hydration = crate::history::ReviewHistoryHydration::for_command(&command);
        for historical in &events[..=index] {
            hydration.observe(historical, &command);
        }
        hydration.capture_active(&state);
        let transition = decide_hydrated(&state, &command, hydration)?;
        if transition.event() != event {
            return Err(replay_error("review event differs from deterministic reduction"));
        }
        state = transition.into_state();
    }
    Ok(state)
}

fn validate_fences(state: &ReviewRunState, command: &ReviewCommand) -> Result<(), ReviewError> {
    if state.phase() == ReviewRunPhase::Terminal {
        return Err(illegal("review aggregate is terminal and fenced closed"));
    }
    if !state.binding().uses_paged_history() && state.used_commands().len() >= 65_535 {
        return Err(reject(
            ReviewErrorKind::LimitExceeded,
            "review command history reached the canonical collection limit",
        ));
    }
    if state.run_id() != command.run_id()
        || state.binding().revision() != command.revision()
        || state.sequence().get() != command.expected_sequence()
        || command.expected_previous_event() != Some(state.last_event_id())
        || command.prior_state_digest() != state.state_digest()
        || (!state.binding().uses_paged_history()
            && state.used_commands().contains(&command.command_id()))
        || matches!(command.kind(), ReviewCommandKind::StartRun { .. })
    {
        return Err(reject(
            ReviewErrorKind::StaleFence,
            "command run, revision, predecessor, state, identity, or lifecycle fence differs",
        ));
    }
    Ok(())
}

fn should_archive(state: &ReviewRunState, command: &ReviewCommand) -> bool {
    if !state.binding().uses_paged_history() {
        return false;
    }
    let command_pressure = state
        .estimated_encoded_bytes()
        .saturating_add(estimated_payload_bytes(command.kind()))
        > state.limits().state_bytes();
    let command_window_full = state.used_commands().len()
        >= usize::from(state.limits().assignments().min(state.limits().cycles()));
    let cycle_window_complete = matches!(command.kind(), ReviewCommandKind::AssignReviewer { .. })
        && state.quorum().complete()
        && state
            .cycles()
            .iter()
            .any(|cycle| cycle.phase() != crate::ReviewCyclePhase::Assigned);
    let finding_window_full = match command.kind() {
        ReviewCommandKind::SubmitReview { submission } => state
            .findings()
            .len()
            .saturating_add(submission.findings().len())
            > state.limits().findings() as usize,
        _ => false,
    };
    command_pressure || command_window_full || cycle_window_complete || finding_window_full
}

#[allow(clippy::too_many_lines)]
pub(crate) fn command_from_event(
    event: &ReviewEvent,
    expected_sequence: u64,
    previous: Option<peritus_types::EventId>,
) -> Result<ReviewCommand, ReviewError> {
    let kind = match event.kind() {
        ReviewEventKind::RunStarted { binding, limits } => {
            ReviewCommandKind::StartRun { binding: binding.clone(), limits: *limits }
        }
        ReviewEventKind::RevisionAdvanced { binding } => {
            ReviewCommandKind::AdvanceRevision { binding: binding.clone() }
        }
        ReviewEventKind::ReviewerAssigned { assignment } => {
            ReviewCommandKind::AssignReviewer { assignment: assignment.clone() }
        }
        ReviewEventKind::ReviewSubmitted { submission } => {
            ReviewCommandKind::SubmitReview { submission: submission.clone() }
        }
        ReviewEventKind::DuplicatesReconciled { canonical, duplicates, reconciliation_digest } => {
            ReviewCommandKind::ReconcileDuplicates {
                canonical: *canonical,
                duplicates: duplicates.clone(),
                reconciliation_digest: *reconciliation_digest,
            }
        }
        ReviewEventKind::FixerResponseRecorded { finding_id, response } => {
            ReviewCommandKind::RecordFixerResponse {
                finding_id: *finding_id,
                response: response.clone(),
            }
        }
        ReviewEventKind::ResolutionConfirmed {
            finding_id,
            reviewer_cycle,
            pending_response_digest,
            evidence,
            confirmation_digest,
        } => ReviewCommandKind::ConfirmResolution {
            finding_id: *finding_id,
            reviewer_cycle: *reviewer_cycle,
            pending_response_digest: *pending_response_digest,
            evidence: evidence.clone(),
            confirmation_digest: *confirmation_digest,
        },
        ReviewEventKind::InvalidationConfirmed {
            finding_id,
            reviewer_cycle,
            pending_response_digest,
            evidence,
            confirmation_digest,
        } => ReviewCommandKind::ConfirmInvalidation {
            finding_id: *finding_id,
            reviewer_cycle: *reviewer_cycle,
            pending_response_digest: *pending_response_digest,
            evidence: evidence.clone(),
            confirmation_digest: *confirmation_digest,
        },
        ReviewEventKind::SupersessionConfirmed {
            finding_id,
            superseding,
            reviewer_cycle,
            pending_response_digest,
            evidence,
            confirmation_digest,
        } => ReviewCommandKind::ConfirmSupersession {
            finding_id: *finding_id,
            superseding: *superseding,
            reviewer_cycle: *reviewer_cycle,
            pending_response_digest: *pending_response_digest,
            evidence: evidence.clone(),
            confirmation_digest: *confirmation_digest,
        },
        ReviewEventKind::WaiverRequested { finding_id, request } => {
            ReviewCommandKind::RequestWaiver { finding_id: *finding_id, request: request.clone() }
        }
        ReviewEventKind::WaiverObserved { waiver } => {
            ReviewCommandKind::ObserveWaiver { waiver: *waiver }
        }
        ReviewEventKind::CycleCancelled { cycle_id } => {
            ReviewCommandKind::CancelCycle { cycle_id: *cycle_id }
        }
        ReviewEventKind::RunCancelled => ReviewCommandKind::CancelRun,
        ReviewEventKind::RunPaused => ReviewCommandKind::PauseRun,
        ReviewEventKind::RunResumed => ReviewCommandKind::ResumeRun,
        ReviewEventKind::BudgetExhausted { reason_digest } => {
            ReviewCommandKind::ExhaustBudget { reason_digest: *reason_digest }
        }
        ReviewEventKind::RunFailed { failure_digest } => {
            ReviewCommandKind::FailRun { failure_digest: *failure_digest }
        }
        ReviewEventKind::RunFinalized => ReviewCommandKind::FinalizeRun,
    };
    ReviewCommand::new(
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

pub fn illegal(detail: &'static str) -> ReviewError {
    reject(ReviewErrorKind::IllegalTransition, detail)
}

fn replay_error(detail: &'static str) -> ReviewError {
    reject(ReviewErrorKind::ReplayMismatch, detail)
}

fn estimated_payload_bytes(kind: &ReviewCommandKind) -> u64 {
    let base = 512_u64;
    match kind {
        ReviewCommandKind::StartRun { binding, .. }
        | ReviewCommandKind::AdvanceRevision { binding } => base
            .saturating_add((binding.required_categories().len() as u64).saturating_mul(32))
            .saturating_add((binding.producer_actors().len() as u64).saturating_mul(16))
            .saturating_add((binding.producer_ancestries().len() as u64).saturating_mul(32)),
        ReviewCommandKind::AssignReviewer { assignment } => {
            base.saturating_add((assignment.categories().len() as u64).saturating_mul(32))
        }
        ReviewCommandKind::SubmitReview { submission } => {
            submission.findings().iter().fold(base, |total, finding| {
                let text = [
                    finding.description(),
                    finding.reproduction(),
                    finding.expected_behavior(),
                    finding.remediation(),
                ]
                .iter()
                .fold(0_u64, |value, text| value.saturating_add(text.len() as u64));
                let paths = finding.locations().iter().fold(0_u64, |value, location| {
                    value.saturating_add(location.path().len() as u64)
                });
                total
                    .saturating_add(text)
                    .saturating_add(paths)
                    .saturating_add((finding.evidence().len() as u64).saturating_mul(16))
                    .saturating_add(512)
            })
        }
        ReviewCommandKind::ReconcileDuplicates { duplicates, .. } => {
            base.saturating_add((duplicates.len() as u64).saturating_mul(16))
        }
        ReviewCommandKind::RecordFixerResponse { response, .. }
        | ReviewCommandKind::RequestWaiver { request: response, .. } => {
            base.saturating_add((response.evidence().len() as u64).saturating_mul(16))
        }
        ReviewCommandKind::ConfirmResolution { evidence, .. }
        | ReviewCommandKind::ConfirmInvalidation { evidence, .. }
        | ReviewCommandKind::ConfirmSupersession { evidence, .. } => {
            base.saturating_add((evidence.len() as u64).saturating_mul(16))
        }
        ReviewCommandKind::ObserveWaiver { .. }
        | ReviewCommandKind::CancelCycle { .. }
        | ReviewCommandKind::CancelRun
        | ReviewCommandKind::PauseRun
        | ReviewCommandKind::ResumeRun
        | ReviewCommandKind::ExhaustBudget { .. }
        | ReviewCommandKind::FailRun { .. }
        | ReviewCommandKind::FinalizeRun => base,
    }
}
