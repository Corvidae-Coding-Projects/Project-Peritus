//! Atomic C0 persistence and checked replay loading for collaboration aggregates.

mod binding;

use peritus_codec::{CodecLimits, decode_message, encode_message, sha256};
use peritus_evidence::revision_digest;
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, CommandResolution, CommittedBatch,
    EventDraft, ExactFrame, HeadExpectation, SqliteJournal, StateInstall,
};
use peritus_types::RunId;

use crate::wire::{CollaborationCommandFrame, CollaborationEventFrame, CollaborationStateFrame};
use crate::{
    CollaborationCommand, CollaborationError, CollaborationErrorKind, CollaborationRecoveryAction,
    CollaborationReplay, CollaborationState, CollaborationTransition,
};

use binding::validate_binding;

/// Journal-owned namespace for current D3 collaboration checkpoints.
pub const COLLABORATION_STATE_NAMESPACE: u16 = 0xD302;
const STATE_KEY_DOMAIN: &[u8] = b"peritus.collaboration.state.v1\0";

/// Derives the dedicated C0 Collaboration aggregate identity.
///
/// # Errors
/// Rejects a run identity that C0 cannot represent.
pub fn collaboration_aggregate_key(run_id: RunId) -> Result<AggregateKey, CollaborationError> {
    let id = AggregateId::new(*run_id.as_bytes()).map_err(|error| {
        CollaborationError::sourced(
            CollaborationErrorKind::Journal,
            CollaborationRecoveryAction::CorrectInput,
            "collaboration run identity cannot be represented by C0",
            error,
        )
    })?;
    Ok(AggregateKey::new(AggregateKind::Collaboration, id))
}

/// Derives the stable domain-separated checkpoint key for a collaboration run.
#[must_use]
pub fn collaboration_state_key(run_id: RunId) -> Vec<u8> {
    let mut key = Vec::with_capacity(STATE_KEY_DOMAIN.len() + 16);
    key.extend_from_slice(STATE_KEY_DOMAIN);
    key.extend_from_slice(run_id.as_bytes());
    key
}

/// Atomically commits one family-74 event and complete family-75 successor checkpoint.
///
/// # Errors
/// Rejects cross-record mismatch, stale head/state CAS, command conflict, or integrity failure.
pub fn commit_collaboration_transition(
    journal: &mut SqliteJournal,
    command: &CollaborationCommand,
    transition: &CollaborationTransition,
) -> Result<CommittedBatch, CollaborationError> {
    validate_binding(command, transition)?;
    let event = transition.event();
    let state = transition.state();
    let aggregate = collaboration_aggregate_key(command.run_id())?;
    let state_key = collaboration_state_key(command.run_id());
    let command_bytes =
        encode_message(&CollaborationCommandFrame::from_command(command), CodecLimits::PRODUCTION)
            .map_err(codec_error)?;
    let event_bytes =
        encode_message(&CollaborationEventFrame(event.clone()), CodecLimits::PRODUCTION)
            .map_err(codec_error)?;
    let state_bytes =
        encode_message(&CollaborationStateFrame::from_state(state), CodecLimits::PRODUCTION)
            .map_err(codec_error)?;
    if canonical_payload_len(&command_bytes)? > state.limits().command_bytes()
        || canonical_payload_len(&event_bytes)? > state.limits().command_bytes()
        || canonical_payload_len(&state_bytes)? > state.limits().state_bytes()
    {
        return Err(binding_error(
            "canonical collaboration command, event, or checkpoint exceeds configured bytes",
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
        journal.state_record(COLLABORATION_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    if head.is_some() != current.is_some() {
        return Err(inconsistent("collaboration journal head/checkpoint presence differs"));
    }
    match head {
        None if command.expected_sequence() != 0 => {
            return Err(binding_error("collaboration genesis expects an existing head"));
        }
        Some(observed)
            if observed.sequence().get() != command.expected_sequence()
                || Some(observed.event_id()) != command.expected_previous_event() =>
        {
            return Err(binding_error("collaboration command fence differs from C0 head"));
        }
        _ => {}
    }
    if current.as_ref().is_some_and(|record| record.revision() != command.expected_sequence()) {
        return Err(inconsistent("collaboration checkpoint revision differs from C0 head"));
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
        COLLABORATION_STATE_NAMESPACE,
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

#[allow(clippy::too_many_arguments, reason = "C0 idempotency bindings remain explicit")]
fn resolve_existing(
    journal: &SqliteJournal,
    command: &CollaborationCommand,
    aggregate: AggregateKey,
    state_key: &[u8],
    event_bytes: &[u8],
    state: &CollaborationState,
    request_digest: peritus_types::Sha256Digest,
) -> Result<Option<CommittedBatch>, CollaborationError> {
    let batch = match journal
        .resolve_command(command.command_id(), request_digest)
        .map_err(journal_error)?
    {
        CommandResolution::Committed(batch) => batch,
        CommandResolution::Conflict { .. } => {
            return Err(binding_error(
                "collaboration command identity conflicts with another canonical request",
            ));
        }
        CommandResolution::DefinitelyAbsent => return Ok(None),
    };
    let checkpoint = journal
        .state_record(COLLABORATION_STATE_NAMESPACE, state_key)
        .map_err(journal_error)?
        .ok_or_else(|| inconsistent("resolved collaboration command has no checkpoint"))?;
    if batch.records().len() != 1
        || batch.records()[0].frame_bytes() != event_bytes
        || batch.records()[0].aggregate() != aggregate
    {
        return Err(inconsistent(
            "resolved collaboration command differs from expected exact event",
        ));
    }
    let observed =
        decode_message::<CollaborationStateFrame>(checkpoint.bytes(), CodecLimits::PRODUCTION)
            .map_err(codec_error)?;
    if checkpoint.revision() == state.sequence().get() && observed.matches_state(state) {
        return Ok(Some(batch));
    }
    if observed.run_id() == state.run_id() && observed.sequence().get() > state.sequence().get() {
        return Err(CollaborationError::new(
            CollaborationErrorKind::Journal,
            CollaborationRecoveryAction::ReplayAggregate,
            "resolved collaboration command belongs to advanced aggregate; replay",
        ));
    }
    Err(inconsistent("resolved collaboration command checkpoint differs from exact successor"))
}

/// Loads canonical D3 events and exact current checkpoint after C0 binding validation.
///
/// # Errors
/// Rejects corruption, wrong families, chain gaps, or checkpoint/head mismatch.
pub fn load_collaboration_replay(
    journal: &SqliteJournal,
    run_id: RunId,
) -> Result<CollaborationReplay, CollaborationError> {
    let store_id = journal.store_id();
    let aggregate = collaboration_aggregate_key(run_id)?;
    let state_key = collaboration_state_key(run_id);
    let records = journal.records_for_aggregate(aggregate).map_err(journal_error)?;
    let state_record =
        journal.state_record(COLLABORATION_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    if records.is_empty() != state_record.is_none() {
        return Err(inconsistent("collaboration events/checkpoint presence differs"));
    }
    let mut events = Vec::with_capacity(records.len());
    for record in records {
        let event = decode_message::<CollaborationEventFrame>(
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
            return Err(binding_error("decoded collaboration event differs from its C0 record"));
        }
        events.push(event);
    }
    let checkpoint = state_record
        .as_ref()
        .map(|record| {
            decode_message::<CollaborationStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
                .map_err(codec_error)
        })
        .transpose()?;
    if let Some(checkpoint) = &checkpoint {
        let last = events
            .last()
            .ok_or_else(|| inconsistent("collaboration checkpoint has no terminal event record"))?;
        let record = state_record
            .as_ref()
            .ok_or_else(|| inconsistent("collaboration checkpoint vanished during validation"))?;
        if checkpoint.run_id() != run_id
            || checkpoint.sequence() != last.sequence()
            || checkpoint.last_event_id() != last.id()
            || checkpoint.revision() != last.revision()
            || checkpoint.state_digest() != last.successor_state_digest()
            || record.revision() != checkpoint.sequence().get()
        {
            return Err(inconsistent(
                "collaboration checkpoint differs from its C0 aggregate head",
            ));
        }
    }
    Ok(CollaborationReplay::from_parts(store_id, events, checkpoint))
}

fn codec_error(error: peritus_codec::CodecError) -> CollaborationError {
    CollaborationError::sourced(
        CollaborationErrorKind::Codec,
        CollaborationRecoveryAction::Quarantine,
        "D3 canonical codec rejected collaboration durability bytes",
        error,
    )
}

fn journal_error(error: peritus_journal::JournalError) -> CollaborationError {
    CollaborationError::sourced(
        CollaborationErrorKind::Journal,
        CollaborationRecoveryAction::ReplayAggregate,
        "C0 rejected or could not observe collaboration transition",
        error,
    )
}

fn canonical_payload_len(bytes: &[u8]) -> Result<u64, CollaborationError> {
    u64::try_from(bytes.len().saturating_sub(peritus_codec::HEADER_LEN))
        .map_err(|_| binding_error("canonical collaboration frame length cannot be represented"))
}

pub fn binding_error(detail: &'static str) -> CollaborationError {
    CollaborationError::new(
        CollaborationErrorKind::Journal,
        CollaborationRecoveryAction::Quarantine,
        detail,
    )
}

pub fn inconsistent(detail: &'static str) -> CollaborationError {
    CollaborationError::new(
        CollaborationErrorKind::Journal,
        CollaborationRecoveryAction::Quarantine,
        detail,
    )
}
