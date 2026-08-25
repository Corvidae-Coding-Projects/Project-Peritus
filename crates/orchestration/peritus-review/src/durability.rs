//! Atomic C0 persistence and checked replay loading for one D2 run aggregate.

mod binding;

use peritus_codec::{CodecLimits, decode_message, encode_message, sha256};
use peritus_evidence::revision_digest;
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, CommandResolution, CommittedBatch,
    EventDraft, ExactFrame, HeadExpectation, SqliteJournal, StateInstall,
};
use peritus_types::RunId;

use crate::wire::{ReviewCommandFrame, ReviewEventFrame, ReviewStateFrame};
use crate::{
    ReviewCommand, ReviewError, ReviewErrorKind, ReviewEvent, ReviewRecoveryAction, ReviewReplay,
    ReviewRunState, ReviewTransition,
};

use binding::validate_binding;

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

/// Atomically commits one family-54 event and its complete family-55 successor checkpoint.
///
/// # Errors
/// Rejects cross-record mismatch, stale head/state CAS, command conflict, or integrity failure.
pub fn commit_review_transition(
    journal: &mut SqliteJournal,
    command: &ReviewCommand,
    transition: &ReviewTransition,
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
    let state_bytes = encode_message(&ReviewStateFrame::from_state(state), CodecLimits::PRODUCTION)
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
    let expectation = head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
    let request = AppendRequest::new(
        journal.store_id(),
        command.command_id(),
        request_digest,
        vec![expectation],
        vec![draft],
        vec![install],
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    journal.append(request.plan().map_err(journal_error)?).map_err(journal_error)
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
    let observed = decode_message::<ReviewStateFrame>(checkpoint.bytes(), CodecLimits::PRODUCTION)
        .map_err(codec_error)?;
    if checkpoint.revision() == state.sequence().get() && observed.matches_state(state) {
        return Ok(Some(batch));
    }
    if observed.run_id() == state.run_id() && observed.sequence().get() > state.sequence().get() {
        return Err(ReviewError::new(
            ReviewErrorKind::Journal,
            ReviewRecoveryAction::ReplayAggregate,
            "resolved review command belongs to an aggregate that has advanced; replay",
        ));
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
    let aggregate = review_aggregate_key(run_id)?;
    let state_key = review_state_key(run_id);
    let records = journal.records_for_aggregate(aggregate).map_err(journal_error)?;
    let state_record =
        journal.state_record(REVIEW_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
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
            decode_message::<ReviewStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
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
    Ok(ReviewReplay::from_parts(store_id, events, checkpoint))
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
