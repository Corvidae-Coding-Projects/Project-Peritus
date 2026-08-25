//! Atomic C0 persistence and checked replay loading for one scheduler aggregate.

mod binding;

use core::fmt;

use peritus_codec::{CodecLimits, decode_message, encode_message, sha256};
use peritus_evidence::revision_digest;
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, CommandResolution, CommittedBatch,
    EventDraft, ExactFrame, HeadExpectation, SqliteJournal, StateInstall, StoreId,
};
use peritus_types::RunId;

use crate::wire::{SchedulerCommandFrame, SchedulerEventFrame, SchedulerStateFrame};
use crate::{
    SchedulerCommand, SchedulerError, SchedulerErrorKind, SchedulerEvent, SchedulerState,
    SchedulerTransition,
};

/// Journal-owned namespace for current scheduler checkpoints.
pub const SCHEDULER_STATE_NAMESPACE: u16 = 0xD301;
const STATE_KEY_DOMAIN: &[u8] = b"peritus.scheduler.state.v1\0";

/// Contiguous canonical events plus their exact atomic checkpoint.
pub struct SchedulerReplay {
    store_id: StoreId,
    events: Vec<SchedulerEvent>,
    checkpoint: Option<SchedulerStateFrame>,
}

impl SchedulerReplay {
    /// Returns durable store identity.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }
    /// Borrows contiguous scheduler events.
    #[must_use]
    pub fn events(&self) -> &[SchedulerEvent] {
        &self.events
    }
    /// Deterministically rebuilds and requires exact complete-checkpoint equality.
    ///
    /// # Errors
    /// Rejects illegal event history or an absent/ahead/behind/different checkpoint.
    pub fn rebuild(&self) -> Result<Option<SchedulerState>, SchedulerError> {
        if self.events.is_empty() {
            return if self.checkpoint.is_none() {
                Ok(None)
            } else {
                Err(inconsistent("scheduler checkpoint exists without events"))
            };
        }
        let state = crate::replay(&self.events)?;
        if !self.checkpoint.as_ref().is_some_and(|frame| frame.matches_state(&state)) {
            return Err(inconsistent("scheduler checkpoint differs from deterministic replay"));
        }
        Ok(Some(state))
    }
}

impl fmt::Debug for SchedulerReplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerReplay")
            .field("store_id", &self.store_id)
            .field("events", &self.events.len())
            .field(
                "checkpoint_sequence",
                &self.checkpoint.as_ref().map(SchedulerStateFrame::sequence),
            )
            .finish_non_exhaustive()
    }
}

/// Derives the dedicated C0 scheduler aggregate key.
///
/// # Errors
/// Rejects a run identity C0 cannot represent.
pub fn scheduler_aggregate_key(run_id: RunId) -> Result<AggregateKey, SchedulerError> {
    let id = AggregateId::new(*run_id.as_bytes()).map_err(|error| {
        SchedulerError::sourced(
            SchedulerErrorKind::Journal,
            crate::SchedulerRecoveryAction::CorrectInput,
            "scheduler run identity cannot be represented by C0",
            error,
        )
    })?;
    Ok(AggregateKey::new(AggregateKind::Scheduler, id))
}

/// Derives the stable domain-separated checkpoint key.
#[must_use]
pub fn scheduler_state_key(run_id: RunId) -> Vec<u8> {
    let mut key = Vec::with_capacity(STATE_KEY_DOMAIN.len() + 16);
    key.extend_from_slice(STATE_KEY_DOMAIN);
    key.extend_from_slice(run_id.as_bytes());
    key
}

/// Atomically appends one family-71 event and installs its family-72 successor checkpoint.
///
/// # Errors
/// Rejects cross-record mismatch, stale head/state CAS, command conflict, limits, or integrity
/// failures. Exact command replay resolves only the exact prior transition.
pub fn commit_scheduler_transition(
    journal: &mut SqliteJournal,
    command: &SchedulerCommand,
    transition: &SchedulerTransition,
) -> Result<CommittedBatch, SchedulerError> {
    binding::validate(command, transition)?;
    let event = transition.event();
    let state = transition.state();
    let aggregate = scheduler_aggregate_key(command.run_id())?;
    let state_key = scheduler_state_key(command.run_id());
    let command_bytes =
        encode_message(&SchedulerCommandFrame::from_command(command), CodecLimits::PRODUCTION)
            .map_err(codec_error)?;
    let event_bytes =
        encode_message(&SchedulerEventFrame::new(event.clone()), CodecLimits::PRODUCTION)
            .map_err(codec_error)?;
    let state_bytes =
        encode_message(&SchedulerStateFrame::from_state(state), CodecLimits::PRODUCTION)
            .map_err(codec_error)?;
    if payload_len(&command_bytes)? > state.binding().limits().payload_bytes()
        || payload_len(&event_bytes)? > state.binding().limits().payload_bytes()
        || payload_len(&state_bytes)? > state.binding().limits().state_bytes()
    {
        return Err(binding_error("scheduler command, event, or state exceeds configured bytes"));
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
        journal.state_record(SCHEDULER_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    if head.is_some() != current.is_some() {
        return Err(inconsistent("scheduler journal head/checkpoint presence differs"));
    }
    match head {
        None if command.expected_sequence() != 0 => {
            return Err(binding_error("scheduler genesis expects an existing head"));
        }
        Some(observed)
            if observed.sequence().get() != command.expected_sequence()
                || Some(observed.event_id()) != command.expected_previous_event() =>
        {
            return Err(binding_error("scheduler command fence differs from C0 head"));
        }
        _ => {}
    }
    if current.as_ref().is_some_and(|record| record.revision() != command.expected_sequence()) {
        return Err(inconsistent("scheduler checkpoint revision differs from C0 head"));
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
        SCHEDULER_STATE_NAMESPACE,
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

#[allow(clippy::too_many_arguments, reason = "all exact idempotency bindings remain explicit")]
fn resolve_existing(
    journal: &SqliteJournal,
    command: &SchedulerCommand,
    aggregate: AggregateKey,
    state_key: &[u8],
    event_bytes: &[u8],
    state: &SchedulerState,
    request_digest: peritus_types::Sha256Digest,
) -> Result<Option<CommittedBatch>, SchedulerError> {
    let batch = match journal
        .resolve_command(command.command_id(), request_digest)
        .map_err(journal_error)?
    {
        CommandResolution::Committed(batch) => batch,
        CommandResolution::Conflict { .. } => {
            return Err(binding_error(
                "scheduler command identity was committed with another canonical digest",
            ));
        }
        CommandResolution::DefinitelyAbsent => return Ok(None),
    };
    let checkpoint = journal
        .state_record(SCHEDULER_STATE_NAMESPACE, state_key)
        .map_err(journal_error)?
        .ok_or_else(|| inconsistent("resolved scheduler command has no checkpoint"))?;
    if batch.records().len() != 1
        || batch.records()[0].frame_bytes() != event_bytes
        || batch.records()[0].aggregate() != aggregate
    {
        return Err(inconsistent("resolved scheduler command differs from expected event"));
    }
    let observed =
        decode_message::<SchedulerStateFrame>(checkpoint.bytes(), CodecLimits::PRODUCTION)
            .map_err(codec_error)?;
    if checkpoint.revision() == state.sequence().get() && observed.matches_state(state) {
        return Ok(Some(batch));
    }
    if observed.run_id() == state.run_id() && observed.sequence().get() > state.sequence().get() {
        return Err(SchedulerError::new(
            SchedulerErrorKind::Journal,
            crate::SchedulerRecoveryAction::ReplayAggregate,
            "resolved scheduler aggregate advanced; replay required",
        ));
    }
    Err(inconsistent("resolved scheduler checkpoint differs from exact successor"))
}

/// Loads typed scheduler events plus their exact current checkpoint.
///
/// # Errors
/// Rejects gaps, wrong families, record/frame mismatch, or head/checkpoint divergence.
pub fn load_scheduler_replay(
    journal: &SqliteJournal,
    run_id: RunId,
) -> Result<SchedulerReplay, SchedulerError> {
    let aggregate = scheduler_aggregate_key(run_id)?;
    let state_key = scheduler_state_key(run_id);
    let records = journal.records_for_aggregate(aggregate).map_err(journal_error)?;
    let state_record =
        journal.state_record(SCHEDULER_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    if records.is_empty() != state_record.is_none() {
        return Err(inconsistent("scheduler events/checkpoint presence differs"));
    }
    let mut events = Vec::with_capacity(records.len());
    for record in records {
        let event =
            decode_message::<SchedulerEventFrame>(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(codec_error)?
                .into_event();
        if event.run_id() != run_id
            || event.sequence() != record.sequence()
            || event.id() != record.event_id()
            || event.command_id() != record.command_id()
            || event.previous_event() != record.previous_event_id()
            || revision_digest(&event.revision()) != record.revision_digest()
        {
            return Err(binding_error("decoded scheduler event differs from C0 record"));
        }
        events.push(event);
    }
    let checkpoint = state_record
        .as_ref()
        .map(|record| {
            decode_message::<SchedulerStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
                .map_err(codec_error)
        })
        .transpose()?;
    if let Some(frame) = &checkpoint {
        let last = events.last().ok_or_else(|| inconsistent("checkpoint has no event"))?;
        let record = state_record.as_ref().ok_or_else(|| inconsistent("checkpoint vanished"))?;
        if frame.run_id() != run_id
            || frame.sequence() != last.sequence()
            || frame.last_event_id() != last.id()
            || frame.revision() != last.revision()
            || frame.state_digest() != last.successor_state_digest()
            || record.revision() != frame.sequence().get()
        {
            return Err(inconsistent("scheduler checkpoint differs from aggregate head"));
        }
    }
    Ok(SchedulerReplay { store_id: journal.store_id(), events, checkpoint })
}

fn codec_error(error: peritus_codec::CodecError) -> SchedulerError {
    SchedulerError::sourced(
        SchedulerErrorKind::Codec,
        crate::SchedulerRecoveryAction::Quarantine,
        "D3 canonical codec rejected scheduler durability bytes",
        error,
    )
}
fn journal_error(error: peritus_journal::JournalError) -> SchedulerError {
    SchedulerError::sourced(
        SchedulerErrorKind::Journal,
        crate::SchedulerRecoveryAction::ReplayAggregate,
        "C0 rejected or could not observe scheduler transition",
        error,
    )
}
fn payload_len(bytes: &[u8]) -> Result<u64, SchedulerError> {
    u64::try_from(bytes.len().saturating_sub(peritus_codec::HEADER_LEN)).map_err(|_| {
        binding_error("scheduler frame length cannot be represented by configured limit")
    })
}
pub fn binding_error(detail: &'static str) -> SchedulerError {
    SchedulerError::new(
        SchedulerErrorKind::Journal,
        crate::SchedulerRecoveryAction::Quarantine,
        detail,
    )
}
pub fn inconsistent(detail: &'static str) -> SchedulerError {
    binding_error(detail)
}
