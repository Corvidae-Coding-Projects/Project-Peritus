//! Atomic C0 persistence and checked replay loading for one D2 run aggregate.

mod binding;
mod index;

use peritus_codec::{CodecLimits, decode_message, encode_message, sha256};
use peritus_evidence::revision_digest;
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, CommandResolution, CommittedBatch,
    EventDraft, ExactFrame, HeadExpectation, ReplayObservation, SqliteJournal, StateInstall,
};
use peritus_types::RunId;

use crate::wire::{ReviewCommandFrame, ReviewEventFrame, ReviewStateFrame};
use crate::{
    ReviewCommand, ReviewError, ReviewErrorKind, ReviewEvent, ReviewRecoveryAction, ReviewReplay,
    ReviewRunState, ReviewTransition,
};

use binding::validate_binding;
use index::ReviewIndexWorkspace;

/// Journal-owned namespace for current D2 review checkpoints.
pub const REVIEW_STATE_NAMESPACE: u16 = 0xD201;
const STATE_KEY_DOMAIN: &[u8] = b"peritus.review.state.v1\0";

/// Derives the dedicated C0 Review aggregate identity.
///
/// # Errors
/// Rejects the reserved zero identity.
pub fn review_aggregate_key(run_id: RunId) -> Result<AggregateKey, ReviewError> {
    let id = AggregateId::new(*run_id.as_bytes()).map_err(|error| {
        ReviewError::sourced(
            ReviewErrorKind::Journal,
            ReviewRecoveryAction::CorrectInput,
            "review run identity cannot be represented by C0",
            error,
        )
    })?;
    Ok(AggregateKey::new(AggregateKind::Review, id))
}

/// Derives the stable domain-separated checkpoint key for a review run.
#[must_use]
pub fn review_state_key(run_id: RunId) -> Vec<u8> {
    let mut key = Vec::with_capacity(STATE_KEY_DOMAIN.len() + 16);
    key.extend_from_slice(STATE_KEY_DOMAIN);
    key.extend_from_slice(run_id.as_bytes());
    key
}

/// Atomically commits one event, its complete checkpoint, and any V2 index mutations.
///
/// # Errors
/// Rejects cross-record mismatch, stale head/state CAS, command conflict, or integrity failure.
pub fn commit_review_transition(
    journal: &mut SqliteJournal,
    command: &ReviewCommand,
    transition: &ReviewTransition,
) -> Result<CommittedBatch, ReviewError> {
    validate_binding(command, transition)?;
    let command_bytes =
        encode_message(&ReviewCommandFrame::from_command(command), CodecLimits::PRODUCTION)
            .map_err(codec_error)?;
    let request_digest = sha256(&command_bytes);
    let aggregate = review_aggregate_key(command.run_id())?;
    let state_key = review_state_key(command.run_id());
    let event_bytes = encode_message(
        &ReviewEventFrame(transition.event().clone()),
        CodecLimits::PRODUCTION,
    )
    .map_err(codec_error)?;
    if let Some(batch) = resolve_existing(
        journal,
        command,
        aggregate,
        &state_key,
        &event_bytes,
        transition.state(),
        request_digest,
    )? {
        return Ok(batch);
    }
    let (workspace, observation) = prepare_transition_index(journal, command, transition)?;
    commit_review_transition_prepared(
        journal,
        command,
        transition,
        workspace,
        observation.as_ref(),
    )
}

fn commit_review_transition_prepared(
    journal: &mut SqliteJournal,
    command: &ReviewCommand,
    transition: &ReviewTransition,
    mut workspace: Option<ReviewIndexWorkspace>,
    observation: Option<&ReplayObservation>,
) -> Result<CommittedBatch, ReviewError> {
    validate_binding(command, transition)?;
    let event = transition.event();
    let state = transition.state();
    let aggregate = review_aggregate_key(command.run_id())?;
    let state_key = review_state_key(command.run_id());
    let command_bytes =
        encode_message(&ReviewCommandFrame::from_command(command), CodecLimits::PRODUCTION)
            .map_err(codec_error)?;
    let event_bytes = encode_message(&ReviewEventFrame(event.clone()), CodecLimits::PRODUCTION)
        .map_err(codec_error)?;
    let state_bytes = crate::wire::encode_state_frame(state, CodecLimits::PRODUCTION)
        .map_err(codec_error)?;
    let command_payload_bytes = canonical_payload_len(&command_bytes)?;
    let event_payload_bytes = canonical_payload_len(&event_bytes)?;
    let state_payload_bytes = canonical_payload_len(&state_bytes)?;
    if command_payload_bytes > state.limits().payload_bytes()
        || event_payload_bytes > state.limits().payload_bytes()
        || state_payload_bytes > state.limits().state_bytes()
    {
        return Err(binding_error(
            "canonical review command, event, or checkpoint exceeds its configured byte limit",
        ));
    }
    let request_digest = sha256(&command_bytes);
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
        journal.state_record(REVIEW_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    if head.is_some() != current.is_some() {
        return Err(inconsistent("review journal head/checkpoint presence differs"));
    }
    match head {
        None if command.expected_sequence() != 0 => {
            return Err(binding_error("review genesis expects an existing head"));
        }
        Some(observed)
            if observed.sequence().get() != command.expected_sequence()
                || Some(observed.event_id()) != command.expected_previous_event() =>
        {
            return Err(binding_error("review command fence differs from the C0 head"));
        }
        _ => {}
    }
    if current.as_ref().is_some_and(|record| record.revision() != command.expected_sequence()) {
        return Err(inconsistent("review checkpoint revision differs from the C0 head"));
    }
    let draft = EventDraft::new(
        aggregate,
        event.sequence(),
        event.id(),
        event.previous_event(),
        ExactFrame::new(event_bytes).map_err(journal_error)?,
        revision_digest(&event.revision()),
        Vec::new(),
    )
    .map_err(journal_error)?;
    let install = StateInstall::new(
        REVIEW_STATE_NAMESPACE,
        state_key,
        current.as_ref().map(peritus_journal::DurableStateRecord::revision),
        state.sequence().get(),
        state_bytes,
    )
    .map_err(journal_error)?;
    let mut installs = vec![install];
    if let Some(workspace) = workspace.as_mut() {
        installs.extend(workspace.prepare_installs(journal)?);
    }
    installs.sort_by(|left, right| {
        (left.namespace(), left.key()).cmp(&(right.namespace(), right.key()))
    });
    let expectation = head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
    let request = AppendRequest::new(
        journal.store_id(),
        command.command_id(),
        request_digest,
        vec![expectation],
        vec![draft],
        installs,
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    let plan = request.plan().map_err(journal_error)?;
    if let Some(observation) = observation {
        match journal.append_observed(plan, observation) {
            Ok((batch, _)) => Ok(batch),
            Err(error) => match resolve_exact_command(journal, command, request_digest)? {
                Some(batch) => Ok(batch),
                None => Err(journal_error(error)),
            },
        }
    } else {
        journal.append(plan).map_err(journal_error)
    }
}

/// Resolves an old exact command first, otherwise point-loads durable facts, reduces, and commits.
///
/// This is the production admission boundary for V2. It never requires a retained command window:
/// C0's durable command row resolves an exact retry even after the aggregate has advanced.
///
/// # Errors
/// Rejects a conflicting command identity, corrupt index/checkpoint association, semantic
/// rejection, or an atomic C0 append failure.
pub fn commit_review_command(
    journal: &mut SqliteJournal,
    command: &ReviewCommand,
) -> Result<CommittedBatch, ReviewError> {
    let command_bytes =
        encode_message(&ReviewCommandFrame::from_command(command), CodecLimits::PRODUCTION)
            .map_err(codec_error)?;
    let request_digest = sha256(&command_bytes);
    if let Some(batch) = resolve_exact_command(journal, command, request_digest)? {
        return Ok(batch);
    }

    let current = load_review_checkpoint(journal, command.run_id())?;
    let Some(state) = current else {
        let transition = crate::start(command)?;
        return commit_review_transition(journal, command, &transition);
    };
    validate_checkpoint_fence(&state, command)?;
    if !state.binding().uses_paged_history() {
        let mut hydration = crate::history::ReviewHistoryHydration::for_command(command);
        hydration.capture_active(&state);
        let transition = crate::reducer::decide_hydrated(&state, command, hydration)?;
        return commit_review_transition(journal, command, &transition);
    }

    let (mut workspace, observation, indexed_state) =
        match ReviewIndexWorkspace::load(journal, &state)? {
            Some(workspace) => (workspace, None, state),
            None => {
                let (replayed, workspace, observation) =
                    reconstruct_review_index(journal, command.run_id())?;
                validate_checkpoint_fence(&replayed, command)?;
                (workspace, Some(observation), replayed)
            }
        };
    let selection = workspace.selection_for_command(journal, command)?;
    let mut hydration = crate::history::ReviewHistoryHydration::from_index(
        command,
        indexed_state.binding(),
        selection,
    );
    hydration.capture_active(&indexed_state);
    let transition =
        crate::reducer::decide_hydrated(&indexed_state, command, hydration)?;
    workspace.apply_transition(journal, transition.event(), transition.state())?;
    commit_review_transition_prepared(
        journal,
        command,
        &transition,
        Some(workspace),
        observation.as_ref(),
    )
}

fn prepare_transition_index(
    journal: &SqliteJournal,
    command: &ReviewCommand,
    transition: &ReviewTransition,
) -> Result<(Option<ReviewIndexWorkspace>, Option<ReplayObservation>), ReviewError> {
    validate_binding(command, transition)?;
    if command.expected_sequence() == 0 {
        let expected = crate::start(command)?;
        validate_precomputed_transition(&expected, transition)?;
        if !transition.state().binding().uses_paged_history() {
            return Ok((None, None));
        }
        let mut reconstructed = crate::history::index::ReconstructedReviewIndex::new();
        reconstructed.observe(transition.event())?;
        let workspace =
            ReviewIndexWorkspace::from_reconstructed(transition.state(), reconstructed)?;
        return Ok((Some(workspace), None));
    }

    let current = load_review_checkpoint(journal, command.run_id())?
        .ok_or_else(|| inconsistent("non-genesis review transition has no current checkpoint"))?;
    validate_checkpoint_fence(&current, command)?;
    if !current.binding().uses_paged_history()
        && !transition.state().binding().uses_paged_history()
    {
        let mut hydration = crate::history::ReviewHistoryHydration::for_command(command);
        hydration.capture_active(&current);
        let expected = crate::reducer::decide_hydrated(&current, command, hydration)?;
        validate_precomputed_transition(&expected, transition)?;
        return Ok((None, None));
    }
    let (mut workspace, observation, indexed_state) =
        match ReviewIndexWorkspace::load(journal, &current)? {
            Some(workspace) => (workspace, None, current),
            None => {
                let (replayed, workspace, observation) =
                    reconstruct_review_index(journal, command.run_id())?;
                validate_checkpoint_fence(&replayed, command)?;
                (workspace, Some(observation), replayed)
            }
        };
    let selection = workspace.selection_for_command(journal, command)?;
    let mut hydration = crate::history::ReviewHistoryHydration::from_index(
        command,
        indexed_state.binding(),
        selection,
    );
    hydration.capture_active(&indexed_state);
    let expected = crate::reducer::decide_hydrated(&indexed_state, command, hydration)?;
    validate_precomputed_transition(&expected, transition)?;
    workspace.apply_transition(journal, transition.event(), transition.state())?;
    Ok((Some(workspace), observation))
}

fn validate_precomputed_transition(
    expected: &ReviewTransition,
    supplied: &ReviewTransition,
) -> Result<(), ReviewError> {
    if expected != supplied {
        return Err(binding_error(
            "precomputed review transition differs from deterministic reduction",
        ));
    }
    Ok(())
}

fn reconstruct_review_index(
    journal: &SqliteJournal,
    run_id: RunId,
) -> Result<(ReviewRunState, ReviewIndexWorkspace, ReplayObservation), ReviewError> {
    let observation = journal.observe_replay().map_err(journal_error)?;
    let (events, checkpoint) = load_review_snapshot(journal, run_id)?;
    let state = checkpoint
        .ok_or_else(|| inconsistent("review index reconstruction has no checkpoint"))?
        .into_state();
    let mut reconstructed = crate::history::index::ReconstructedReviewIndex::new();
    for event in &events {
        reconstructed.observe(event)?;
    }
    reconstructed.validate_checkpoint(&state)?;
    let workspace = ReviewIndexWorkspace::from_reconstructed(&state, reconstructed)?;
    Ok((state, workspace, observation))
}

fn validate_checkpoint_fence(
    state: &ReviewRunState,
    command: &ReviewCommand,
) -> Result<(), ReviewError> {
    if state.run_id() != command.run_id()
        || state.binding().revision() != command.revision()
        || state.sequence().get() != command.expected_sequence()
        || command.expected_previous_event() != Some(state.last_event_id())
        || command.prior_state_digest() != state.state_digest()
    {
        return Err(crate::error::reject(
            ReviewErrorKind::StaleFence,
            "review command differs from the current bounded checkpoint",
        ));
    }
    Ok(())
}

fn resolve_exact_command(
    journal: &SqliteJournal,
    command: &ReviewCommand,
    request_digest: peritus_types::Sha256Digest,
) -> Result<Option<CommittedBatch>, ReviewError> {
    match journal
        .resolve_command(command.command_id(), request_digest)
        .map_err(journal_error)?
    {
        CommandResolution::Committed(batch) => {
            validate_exact_retry(command, &batch)?;
            Ok(Some(batch))
        }
        CommandResolution::Conflict { .. } => Err(binding_error(
            "review command identity was already committed with another canonical digest",
        )),
        CommandResolution::DefinitelyAbsent => Ok(None),
    }
}

/// Loads and validates only the current bounded checkpoint and its exact C0 head.
///
/// # Errors
/// Rejects head/checkpoint absence mismatch, corrupt bytes, or an inexact current frontier.
pub fn load_review_checkpoint(
    journal: &SqliteJournal,
    run_id: RunId,
) -> Result<Option<ReviewRunState>, ReviewError> {
    let aggregate = review_aggregate_key(run_id)?;
    let key = review_state_key(run_id);
    let head = journal.head(aggregate).map_err(journal_error)?;
    let record = journal
        .state_record(REVIEW_STATE_NAMESPACE, &key)
        .map_err(journal_error)?;
    match (head, record) {
        (None, None) => Ok(None),
        (Some(head), Some(record)) => {
            let frame = crate::wire::decode_state_frame(record.bytes(), CodecLimits::PRODUCTION)
                .map_err(codec_error)?;
            let state = frame.into_state();
            if state.run_id() != run_id
                || state.sequence() != head.sequence()
                || state.last_event_id() != head.event_id()
                || state.sequence().get() != record.revision()
            {
                return Err(inconsistent(
                    "review checkpoint differs from its exact current aggregate head",
                ));
            }
            Ok(Some(state))
        }
        _ => Err(inconsistent("review journal head/checkpoint presence differs")),
    }
}

/// Loads one exact sequence-indexed review event without retaining the aggregate prefix.
///
/// # Errors
/// Rejects a malformed sequence, corrupt C0 row, or decoded domain mismatch.
pub fn load_review_event(
    journal: &SqliteJournal,
    run_id: RunId,
    sequence: u64,
) -> Result<Option<ReviewEvent>, ReviewError> {
    if sequence == 0 {
        return Ok(None);
    }
    let aggregate = review_aggregate_key(run_id)?;
    let records = journal
        .aggregate_events_after(aggregate, sequence - 1, 1)
        .map_err(journal_error)?;
    let Some(record) = records.into_iter().next() else {
        return Ok(None);
    };
    if record.sequence().get() != sequence {
        return Ok(None);
    }
    let event = decode_message::<ReviewEventFrame>(
        record.frame_bytes(),
        CodecLimits::PRODUCTION,
    )
    .map_err(codec_error)?
    .into_event();
    if event.run_id() != run_id
        || event.sequence() != record.sequence()
        || event.id() != record.event_id()
        || event.command_id() != record.command_id()
        || event.previous_event() != record.previous_event_id()
        || revision_digest(&event.revision()) != record.revision_digest()
    {
        return Err(inconsistent(
            "sequence-indexed review event differs from its immutable C0 record",
        ));
    }
    Ok(Some(event))
}

fn validate_exact_retry(
    command: &ReviewCommand,
    batch: &CommittedBatch,
) -> Result<(), ReviewError> {
    let record = batch
        .records()
        .first()
        .filter(|_| batch.records().len() == 1)
        .ok_or_else(|| inconsistent("resolved review command has no exact single event"))?;
    let aggregate = review_aggregate_key(command.run_id())?;
    if record.aggregate() != aggregate
        || record.event_id() != command.event_id()
        || record.command_id() != command.command_id()
    {
        return Err(inconsistent(
            "resolved review command belongs to another event or aggregate",
        ));
    }
    let event = decode_message::<ReviewEventFrame>(
        record.frame_bytes(),
        CodecLimits::PRODUCTION,
    )
    .map_err(codec_error)?
    .into_event();
    let expected_sequence = event.sequence().get().checked_sub(1).ok_or_else(|| {
        inconsistent("resolved review event has an invalid predecessor sequence")
    })?;
    let reconstructed = crate::reducer::command_from_event(
        &event,
        expected_sequence,
        event.previous_event(),
    )?;
    if &reconstructed != command {
        return Err(inconsistent(
            "resolved review command differs from its immutable event semantics",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, reason = "all C0 idempotency bindings remain explicit")]
fn resolve_existing(
    journal: &SqliteJournal,
    command: &ReviewCommand,
    aggregate: AggregateKey,
    state_key: &[u8],
    event_bytes: &[u8],
    state: &ReviewRunState,
    request_digest: peritus_types::Sha256Digest,
) -> Result<Option<CommittedBatch>, ReviewError> {
    let batch = match journal
        .resolve_command(command.command_id(), request_digest)
        .map_err(journal_error)?
    {
        CommandResolution::Committed(batch) => batch,
        CommandResolution::Conflict { .. } => {
            return Err(binding_error(
                "review command identity was already committed with another canonical digest",
            ));
        }
        CommandResolution::DefinitelyAbsent => return Ok(None),
    };
    let checkpoint = journal
        .state_record(REVIEW_STATE_NAMESPACE, state_key)
        .map_err(journal_error)?
        .ok_or_else(|| inconsistent("resolved review command has no checkpoint"))?;
    if batch.records().len() != 1
        || batch.records()[0].frame_bytes() != event_bytes
        || batch.records()[0].aggregate() != aggregate
    {
        return Err(inconsistent("resolved review command differs from its expected exact event"));
    }
    let observed = crate::wire::decode_state_frame(checkpoint.bytes(), CodecLimits::PRODUCTION)
        .map_err(codec_error)?;
    if checkpoint.revision() == state.sequence().get() && observed.matches_state(state) {
        return Ok(Some(batch));
    }
    if observed.run_id() == state.run_id() && observed.sequence().get() > state.sequence().get() {
        return Ok(Some(batch));
    }
    Err(inconsistent("resolved review command checkpoint differs from its exact successor state"))
}

/// Loads typed D2 events and the exact current checkpoint after C0 binding validation.
///
/// # Errors
/// Rejects corruption, wrong frame families, chain gaps, or checkpoint/head mismatch.
pub fn load_review_replay(
    journal: &SqliteJournal,
    run_id: RunId,
) -> Result<ReviewReplay, ReviewError> {
    let store_id = journal.store_id();
    let (events, checkpoint) = load_review_snapshot(journal, run_id)?;
    Ok(ReviewReplay::from_parts(store_id, events, checkpoint))
}

fn load_review_snapshot(
    journal: &SqliteJournal,
    run_id: RunId,
) -> Result<(Vec<ReviewEvent>, Option<ReviewStateFrame>), ReviewError> {
    let aggregate = review_aggregate_key(run_id)?;
    let state_key = review_state_key(run_id);
    let (records, state_record) = journal
        .aggregate_checkpoint_snapshot(aggregate, REVIEW_STATE_NAMESPACE, &state_key)
        .map_err(journal_error)?
        .into_parts();
    if records.is_empty() != state_record.is_none() {
        return Err(inconsistent("review events/checkpoint presence differs"));
    }
    let mut events = Vec::with_capacity(records.len());
    for record in records {
        let event =
            decode_message::<ReviewEventFrame>(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(codec_error)?
                .into_event();
        if event.run_id() != run_id
            || event.sequence() != record.sequence()
            || event.id() != record.event_id()
            || event.command_id() != record.command_id()
            || event.previous_event() != record.previous_event_id()
            || revision_digest(&event.revision()) != record.revision_digest()
        {
            return Err(binding_error("decoded review event differs from its C0 record"));
        }
        events.push(event);
    }
    let checkpoint = state_record
        .as_ref()
        .map(|record| {
            crate::wire::decode_state_frame(record.bytes(), CodecLimits::PRODUCTION)
                .map_err(codec_error)
        })
        .transpose()?;
    if let Some(checkpoint) = &checkpoint {
        let last = events
            .last()
            .ok_or_else(|| inconsistent("review checkpoint has no terminal event record"))?;
        let record = state_record
            .as_ref()
            .ok_or_else(|| inconsistent("review checkpoint vanished during validation"))?;
        if checkpoint.run_id() != run_id
            || checkpoint.sequence() != last.sequence()
            || checkpoint.last_event_id() != last.id()
            || checkpoint.revision() != checkpoint_revision(last)
            || checkpoint.state_digest() != last.successor_state_digest()
            || record.revision() != checkpoint.sequence().get()
        {
            return Err(inconsistent("review checkpoint differs from its C0 aggregate head"));
        }
    }
    Ok((events, checkpoint))
}

const fn checkpoint_revision(event: &ReviewEvent) -> peritus_types::RevisionTuple {
    match event.kind() {
        crate::ReviewEventKind::RevisionAdvanced { binding } => binding.revision(),
        _ => event.revision(),
    }
}

fn codec_error(error: peritus_codec::CodecError) -> ReviewError {
    ReviewError::sourced(
        ReviewErrorKind::Codec,
        ReviewRecoveryAction::Quarantine,
        "D2 canonical codec rejected review durability bytes",
        error,
    )
}

fn journal_error(error: peritus_journal::JournalError) -> ReviewError {
    ReviewError::sourced(
        ReviewErrorKind::Journal,
        ReviewRecoveryAction::ReplayAggregate,
        "C0 rejected or could not observe the review transition",
        error,
    )
}

fn canonical_payload_len(bytes: &[u8]) -> Result<u64, ReviewError> {
    u64::try_from(bytes.len().saturating_sub(peritus_codec::HEADER_LEN)).map_err(|_| {
        binding_error("canonical review frame length cannot be represented by its configured limit")
    })
}

pub fn binding_error(detail: &'static str) -> ReviewError {
    ReviewError::new(ReviewErrorKind::Journal, ReviewRecoveryAction::Quarantine, detail)
}

pub fn inconsistent(detail: &'static str) -> ReviewError {
    ReviewError::new(ReviewErrorKind::Journal, ReviewRecoveryAction::Quarantine, detail)
}
