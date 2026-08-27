//! Atomic C0 persistence and checked replay loading for one D1 aggregate.

mod binding;
mod lifecycle;

pub use lifecycle::commit_gate_lifecycle_transition;

use core::fmt;

use peritus_codec::{CodecLimits, decode_message, encode_message, sha256};
use peritus_evidence::revision_digest;
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, ArtifactDependency, CommandResolution,
    CommittedBatch, EventDraft, ExactFrame, HeadExpectation, SqliteJournal, StateInstall, StoreId,
};
use peritus_types::RunId;

use crate::wire::{GateCommandFrame, GateEventFrame, GateStateFrame};
use crate::{
    GateCommand, GateError, GateErrorKind, GateEvent, GateEventKind, GatePlan, GateRecoveryAction,
    GateRunState, GateTransition,
};

use binding::validate_binding;

/// Journal-owned namespace for current D1 aggregate checkpoints.
pub const GATE_STATE_NAMESPACE: u16 = 0xD101;
const STATE_KEY_DOMAIN: &[u8] = b"peritus.gate.state.v1\0";

/// Whether this call appended the transition or resolved an already committed command.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateCommitDisposition {
    /// This call atomically appended the event and installed its checkpoint.
    Committed,
    /// C0 returned the exact event and exact checkpoint committed by an earlier call.
    Resolved,
}

/// One checked C0 commit observation and whether it was newly appended.
pub struct GateCommitObservation {
    batch: CommittedBatch,
    disposition: GateCommitDisposition,
}

impl GateCommitObservation {
    /// Returns whether this call newly committed or only resolved the transition.
    #[must_use]
    pub const fn disposition(&self) -> GateCommitDisposition {
        self.disposition
    }

    /// Returns the checked committed C0 batch.
    #[must_use]
    pub fn into_batch(self) -> CommittedBatch {
        self.batch
    }
}

/// Checked D1 chain and its atomically installed cache checkpoint.
pub struct GateReplay {
    store_id: StoreId,
    events: Vec<GateEvent>,
    checkpoint: Option<GateStateFrame>,
}

impl GateReplay {
    /// Returns the durable journal store that produced this replay observation.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }

    /// Borrows contiguous canonical D1 events.
    #[must_use]
    pub fn events(&self) -> &[GateEvent] {
        &self.events
    }

    /// Rebuilds semantic state and requires the checkpoint to match every field.
    ///
    /// # Errors
    /// Rejects an absent/mismatched plan, event chain, or checkpoint.
    pub fn rebuild(&self, plan: &GatePlan) -> Result<Option<GateRunState>, GateError> {
        if self.events.is_empty() {
            return if self.checkpoint.is_none() {
                Ok(None)
            } else {
                Err(inconsistent("gate checkpoint exists without events"))
            };
        }
        let state = crate::replay(plan, &self.events)?;
        if !self.checkpoint.as_ref().is_some_and(|checkpoint| checkpoint.matches_state(&state)) {
            return Err(inconsistent("gate checkpoint differs from deterministic event replay"));
        }
        Ok(Some(state))
    }
}

impl fmt::Debug for GateReplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GateReplay")
            .field("store_id", &self.store_id)
            .field("events", &self.events.len())
            .field("checkpoint_sequence", &self.checkpoint.as_ref().map(GateStateFrame::sequence))
            .finish_non_exhaustive()
    }
}

/// Derives the dedicated C0 Gate aggregate identity.
///
/// # Errors
/// Rejects the reserved zero identity.
pub fn gate_aggregate_key(run_id: RunId) -> Result<AggregateKey, GateError> {
    let id = AggregateId::new(*run_id.as_bytes()).map_err(|error| {
        GateError::sourced(
            GateErrorKind::Journal,
            GateRecoveryAction::CorrectInput,
            "gate run identity cannot be represented by C0",
            error,
        )
    })?;
    Ok(AggregateKey::new(AggregateKind::Gate, id))
}

/// Derives the stable checkpoint key for a run.
#[must_use]
pub fn gate_state_key(run_id: RunId) -> Vec<u8> {
    let mut key = Vec::with_capacity(STATE_KEY_DOMAIN.len() + 16);
    key.extend_from_slice(STATE_KEY_DOMAIN);
    key.extend_from_slice(run_id.as_bytes());
    key
}

/// Atomically commits one accepted event and its complete successor checkpoint.
///
/// # Errors
/// Rejects cross-record mismatch, stale C0 head/state, missing artifacts, codec failure, or journal
/// integrity failure.
pub fn commit_gate_transition(
    journal: &mut SqliteJournal,
    command: &GateCommand,
    transition: &GateTransition,
) -> Result<CommittedBatch, GateError> {
    commit_gate_transition_observed(journal, command, transition)
        .map(GateCommitObservation::into_batch)
}

/// Atomically commits a transition and distinguishes a new append from exact resolution.
///
/// # Errors
/// Rejects the same mismatches as [`commit_gate_transition`], including a resolved command whose
/// aggregate checkpoint has already advanced beyond the caller's local transition.
pub fn commit_gate_transition_observed(
    journal: &mut SqliteJournal,
    command: &GateCommand,
    transition: &GateTransition,
) -> Result<GateCommitObservation, GateError> {
    validate_binding(command, transition)?;
    let event = transition.event();
    let state = transition.state();
    let aggregate = gate_aggregate_key(command.run_id())?;
    let state_key = gate_state_key(command.run_id());
    let command_bytes =
        encode_message(&GateCommandFrame::from_command(command), CodecLimits::PRODUCTION)
            .map_err(codec_error)?;
    let event_bytes = encode_message(&GateEventFrame(event.clone()), CodecLimits::PRODUCTION)
        .map_err(codec_error)?;
    let state_bytes = encode_message(&GateStateFrame::from_state(state), CodecLimits::PRODUCTION)
        .map_err(codec_error)?;
    let request_digest = sha256(&command_bytes);
    if let Some(observation) = resolve_existing(
        journal,
        command,
        aggregate,
        &state_key,
        &event_bytes,
        state,
        request_digest,
    )? {
        return Ok(observation);
    }
    let head = journal.head(aggregate).map_err(journal_error)?;
    let current = journal.state_record(GATE_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    if head.is_some() != current.is_some() {
        return Err(inconsistent("gate journal head/checkpoint presence differs"));
    }
    match head {
        None if command.expected_sequence() != 0 => {
            return Err(binding_error("gate genesis expects an existing head"));
        }
        Some(observed)
            if observed.sequence().get() != command.expected_sequence()
                || Some(observed.event_id()) != command.expected_previous_event() =>
        {
            return Err(binding_error("gate command fence differs from the C0 head"));
        }
        _ => {}
    }
    if current.as_ref().is_some_and(|record| record.revision() != command.expected_sequence()) {
        return Err(inconsistent("gate checkpoint revision differs from the C0 head"));
    }
    let frame = ExactFrame::new(event_bytes).map_err(journal_error)?;
    let draft = EventDraft::new(
        aggregate,
        event.sequence(),
        event.id(),
        event.previous_event(),
        frame,
        revision_digest(&event.revision()),
        Vec::new(),
    )
    .map_err(journal_error)?;
    let install = StateInstall::new(
        GATE_STATE_NAMESPACE,
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
        artifact_dependencies(event.kind()),
        None,
        None,
        Vec::new(),
    );
    let plan = request.plan().map_err(journal_error)?;
    let batch = journal.append(plan).map_err(journal_error)?;
    Ok(GateCommitObservation { batch, disposition: GateCommitDisposition::Committed })
}

#[allow(clippy::too_many_arguments, reason = "all C0 idempotency bindings remain explicit")]
fn resolve_existing(
    journal: &SqliteJournal,
    command: &GateCommand,
    aggregate: AggregateKey,
    state_key: &[u8],
    event_bytes: &[u8],
    state: &GateRunState,
    request_digest: peritus_types::Sha256Digest,
) -> Result<Option<GateCommitObservation>, GateError> {
    let resolution =
        journal.resolve_command(command.command_id(), request_digest).map_err(journal_error)?;
    let batch = match resolution {
        CommandResolution::Committed(batch) => batch,
        CommandResolution::Conflict { .. } => {
            return Err(binding_error(
                "gate command identity was already committed with another digest",
            ));
        }
        CommandResolution::DefinitelyAbsent => return Ok(None),
    };
    let checkpoint = journal
        .state_record(GATE_STATE_NAMESPACE, state_key)
        .map_err(journal_error)?
        .ok_or_else(|| inconsistent("resolved gate command has no checkpoint"))?;
    let exact_event = batch.records().len() == 1
        && batch.records()[0].frame_bytes() == event_bytes
        && batch.records()[0].aggregate() == aggregate;
    if !exact_event {
        return Err(inconsistent("resolved gate command differs from its expected transition"));
    }
    let observed = decode_message::<GateStateFrame>(checkpoint.bytes(), CodecLimits::PRODUCTION)
        .map_err(codec_error)?;
    if checkpoint.revision() == state.sequence().get() && observed.matches_state(state) {
        return Ok(Some(GateCommitObservation {
            batch,
            disposition: GateCommitDisposition::Resolved,
        }));
    }
    if observed.run_id() == state.run_id()
        && observed.revision() == state.revision()
        && observed.sequence().get() > state.sequence().get()
    {
        return Err(replay_required(
            "resolved gate command belongs to an aggregate that has advanced; replay",
        ));
    }
    Err(inconsistent("resolved gate command checkpoint differs from its exact successor state"))
}

/// Loads typed D1 events and the current checkpoint after validating C0 record bindings.
///
/// # Errors
/// Rejects corruption, wrong frame families, gaps, stale revision digests, or checkpoint/head
/// inconsistency.
pub fn load_gate_replay(journal: &SqliteJournal, run_id: RunId) -> Result<GateReplay, GateError> {
    let store_id = journal.store_id();
    let aggregate = gate_aggregate_key(run_id)?;
    let state_key = gate_state_key(run_id);
    let records = journal.records_for_aggregate(aggregate).map_err(journal_error)?;
    let state_record =
        journal.state_record(GATE_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    if records.is_empty() != state_record.is_none() {
        return Err(inconsistent("gate events/checkpoint presence differs"));
    }
    let mut events = Vec::with_capacity(records.len());
    for record in records {
        let event = decode_message::<GateEventFrame>(record.frame_bytes(), CodecLimits::PRODUCTION)
            .map_err(codec_error)?
            .into_event();
        if event.run_id() != run_id
            || event.sequence() != record.sequence()
            || event.id() != record.event_id()
            || event.command_id() != record.command_id()
            || event.previous_event() != record.previous_event_id()
            || revision_digest(&event.revision()) != record.revision_digest()
        {
            return Err(binding_error("decoded gate event differs from its C0 record"));
        }
        events.push(event);
    }
    let checkpoint = state_record
        .as_ref()
        .map(|record| {
            decode_message::<GateStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
                .map_err(codec_error)
        })
        .transpose()?;
    if let Some(checkpoint) = &checkpoint {
        let Some(last) = events.last() else {
            return Err(inconsistent("gate checkpoint has no terminal event record"));
        };
        let record = state_record.as_ref().ok_or_else(|| {
            inconsistent("gate checkpoint observation disappeared during validation")
        })?;
        if checkpoint.run_id() != run_id
            || checkpoint.sequence() != last.sequence()
            || checkpoint.last_event_id() != last.id()
            || checkpoint.revision() != last.revision()
            || checkpoint.state_digest() != last.successor_state_digest()
            || record.revision() != checkpoint.sequence().get()
        {
            return Err(inconsistent("gate checkpoint differs from its C0 aggregate head"));
        }
    }
    Ok(GateReplay { store_id, events, checkpoint })
}

fn artifact_dependencies(kind: &GateEventKind) -> Vec<ArtifactDependency> {
    let mut digests = match kind {
        GateEventKind::ResultObserved { result, .. } => {
            result.artifacts().iter().map(crate::GateArtifact::digest).collect()
        }
        GateEventKind::EvidencePublished { receipt, .. } => vec![receipt.manifest_digest()],
        _ => Vec::new(),
    };
    digests.sort_unstable();
    digests.dedup();
    digests.into_iter().map(ArtifactDependency::new).collect()
}

fn codec_error(error: peritus_codec::CodecError) -> GateError {
    GateError::sourced(
        GateErrorKind::Codec,
        GateRecoveryAction::Quarantine,
        "D1 canonical codec rejected gate durability bytes",
        error,
    )
}

fn journal_error(error: peritus_journal::JournalError) -> GateError {
    GateError::sourced(
        GateErrorKind::Journal,
        GateRecoveryAction::ReplayAggregate,
        "C0 rejected or could not observe the gate transition",
        error,
    )
}

fn binding_error(detail: &'static str) -> GateError {
    GateError::new(GateErrorKind::Journal, GateRecoveryAction::Quarantine, detail)
}

fn inconsistent(detail: &'static str) -> GateError {
    GateError::new(GateErrorKind::Journal, GateRecoveryAction::Quarantine, detail)
}

fn replay_required(detail: &'static str) -> GateError {
    GateError::new(GateErrorKind::Journal, GateRecoveryAction::ReplayAggregate, detail)
}
