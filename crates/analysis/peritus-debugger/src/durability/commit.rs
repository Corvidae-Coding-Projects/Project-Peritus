//! Atomic family-83 event, family-84 checkpoint, artifact, and outbox persistence.

mod claim;
mod outbox;

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_journal::{
    AppendRequest, CommandResolution, CommittedBatch, EventDraft, ExactFrame, HeadExpectation,
    SqliteJournal, StateInstall,
};
use peritus_types::{CommandId, EventSequence};

use crate::{
    DebuggerCommand, DebuggerError, DebuggerErrorKind, DebuggerEvent, DebuggerJobId,
    DebuggerOperation, DebuggerRecovery, DebuggerState, DebuggerTransition, apply_event,
    wire::{DebuggerCommandFrame, DebuggerEventFrame, DebuggerStateFrame},
};

use super::{
    CommittedDebuggerOperation, DEBUGGER_RECEIPT_NAMESPACE, DEBUGGER_STATE_NAMESPACE,
    DebuggerCommitMode, DebuggerDirectiveClaim, DebuggerOperationReceipt, binding,
    debugger_aggregate_key, debugger_state_key, load_debugger_replay,
    receipt::{claim_receipt, receipt_key},
};

/// Atomically appends an ordinary transition and installs its complete checkpoint.
///
/// Model-attempt starts and effect settlements require the explicitly fenced variants.
///
/// # Errors
/// Rejects cross-record mismatch, stale CAS, a reused command identity, or an effect command.
pub fn commit_debugger_transition(
    journal: &mut SqliteJournal,
    command: &DebuggerCommand,
    transition: &DebuggerTransition,
) -> Result<CommittedDebuggerOperation, DebuggerError> {
    commit(journal, command, transition, CommitMode::Ordinary)
}

/// Commits that a claimed model directive is about to perform provider I/O.
///
/// The directive remains claimed; its success or failure must subsequently be committed with
/// [`commit_debugger_settlement`] so acknowledgement and aggregate settlement are atomic.
///
/// # Errors
/// Rejects a non-start transition or a claim that differs from the model attempt.
pub fn commit_debugger_claimed_transition(
    journal: &mut SqliteJournal,
    command: &DebuggerCommand,
    transition: &DebuggerTransition,
    claim: super::ModelDirectiveClaim,
) -> Result<CommittedDebuggerOperation, DebuggerError> {
    commit(journal, command, transition, CommitMode::Claimed(DebuggerDirectiveClaim::Model(claim)))
}

/// Atomically commits an effect result and acknowledges its exact claimed C0 directive.
///
/// # Errors
/// Rejects an unrelated claim, stale fence, non-settlement transition, or ordinary integrity
/// failure.
pub fn commit_debugger_settlement(
    journal: &mut SqliteJournal,
    command: &DebuggerCommand,
    transition: &DebuggerTransition,
    claim: impl Into<DebuggerDirectiveClaim>,
) -> Result<CommittedDebuggerOperation, DebuggerError> {
    commit(journal, command, transition, CommitMode::Settlement(claim.into()))
}

#[derive(Clone, Copy)]
pub(super) enum CommitMode {
    Ordinary,
    Claimed(DebuggerDirectiveClaim),
    Settlement(DebuggerDirectiveClaim),
}

impl CommitMode {
    const fn receipt_mode(self) -> DebuggerCommitMode {
        match self {
            Self::Ordinary => DebuggerCommitMode::Ordinary,
            Self::Claimed(_) => DebuggerCommitMode::Claimed,
            Self::Settlement(_) => DebuggerCommitMode::Settlement,
        }
    }

    const fn claim(self) -> Option<DebuggerDirectiveClaim> {
        match self {
            Self::Ordinary => None,
            Self::Claimed(claim) | Self::Settlement(claim) => Some(claim),
        }
    }

    const fn is_claim_bound(self) -> bool {
        !matches!(self, Self::Ordinary)
    }
}

fn commit(
    journal: &mut SqliteJournal,
    command: &DebuggerCommand,
    transition: &DebuggerTransition,
    mode: CommitMode,
) -> Result<CommittedDebuggerOperation, DebuggerError> {
    binding::validate(command, transition)?;
    claim::validate_mode(command, transition.state(), mode)?;
    let event = transition.event();
    let state = transition.state();
    let aggregate = debugger_aggregate_key(command.job_id())?;
    let state_key = debugger_state_key(command.job_id());
    let command_bytes = encode_message(
        &DebuggerCommandFrame::from_command(command).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    let event_bytes = encode_message(
        &DebuggerEventFrame::from_event(event).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    let state_bytes =
        encode_message(&DebuggerStateFrame::from_state(state), CodecLimits::PRODUCTION)
            .map_err(codec)?;
    let base_digest = peritus_codec::sha256(&command_bytes);
    let request_digest = match mode {
        CommitMode::Ordinary => base_digest,
        CommitMode::Claimed(claim) => claim::claimed_digest(base_digest, claim)?,
        CommitMode::Settlement(claim) => claim::acknowledged_digest(base_digest, claim)?,
    };
    let receipt = operation_receipt(
        command,
        event,
        peritus_codec::sha256(&event_bytes),
        base_digest,
        request_digest,
        mode,
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
        mode,
    )? {
        return Ok(operation);
    }
    let head = journal.head(aggregate).map_err(journal_error)?;
    let current =
        journal.state_record(DEBUGGER_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    validate_current(command, head, current.as_ref())?;
    let draft = EventDraft::new(
        aggregate,
        EventSequence::new(event.sequence())
            .map_err(|_| binding::binding("debugger event sequence is zero"))?,
        event.id(),
        event.previous_event(),
        ExactFrame::new(event_bytes).map_err(journal_error)?,
        peritus_evidence::revision_digest(state.revision()),
        Vec::new(),
    )
    .map_err(journal_error)?;
    let installs = vec![
        StateInstall::new(
            DEBUGGER_STATE_NAMESPACE,
            state_key,
            current.as_ref().map(peritus_journal::DurableStateRecord::revision),
            state.sequence(),
            state_bytes,
        )
        .map_err(journal_error)?,
        StateInstall::new(
            DEBUGGER_RECEIPT_NAMESPACE,
            receipt_key(command.command_id()),
            None,
            1,
            receipt.canonical_bytes()?,
        )
        .map_err(journal_error)?,
    ];
    let expectation = head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
    let dependencies = outbox::artifact_dependencies(event.kind());
    let outbox = outbox::transition_outbox(command, state)?;
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
        installs,
        dependencies,
        None,
        None,
        outbox,
    );
    let request = if let CommitMode::Settlement(claim) = mode {
        request
            .with_outbox_acknowledgements(vec![claim::acknowledgement(claim)?])
            .map_err(journal_error)?
    } else {
        request
    };
    let _accepted = journal.append(request.plan().map_err(journal_error)?).map_err(journal_error)?;
    load_debugger_operation(journal, command.job_id(), command.command_id())?
        .ok_or_else(|| recovery("accepted debugger operation has no durable receipt"))
}

fn validate_current(
    command: &DebuggerCommand,
    head: Option<peritus_journal::AggregateHead>,
    current: Option<&peritus_journal::DurableStateRecord>,
) -> Result<(), DebuggerError> {
    if head.is_some() != current.is_some() {
        return Err(recovery("debugger journal head/checkpoint presence differs"));
    }
    match head {
        None if command.expected_sequence() != 0 => {
            return Err(binding::binding("debugger genesis expects an existing C0 head"));
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
        return Err(recovery("debugger checkpoint revision differs from C0 head"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, reason = "complete idempotency evidence remains explicit")]
fn resolve_existing(
    journal: &SqliteJournal,
    command: &DebuggerCommand,
    aggregate: peritus_journal::AggregateKey,
    event: &DebuggerEvent,
    event_bytes: &[u8],
    state: &DebuggerState,
    request_digest: peritus_types::Sha256Digest,
    expected_receipt: DebuggerOperationReceipt,
    mode: CommitMode,
) -> Result<Option<CommittedDebuggerOperation>, DebuggerError> {
    let exact_request = match journal
        .resolve_command(command.command_id(), request_digest)
        .map_err(journal_error)?
    {
        CommandResolution::Committed(_) => true,
        CommandResolution::Conflict { .. } => {
            if !mode.is_claim_bound() {
                return Err(conflict(
                    "command identity was committed with another exact request digest",
                ));
            }
            false
        }
        CommandResolution::DefinitelyAbsent => return Ok(None),
    };
    let operation = load_debugger_operation(journal, command.job_id(), command.command_id())?
        .ok_or_else(|| recovery("resolved debugger command has no retained operation"))?;
    if operation.batch().records().len() != 1
        || operation.batch().records()[0].frame_bytes() != event_bytes
        || operation.batch().records()[0].aggregate() != aggregate
        || operation.event() != event
        || operation.historical_state() != state
    {
        return Err(conflict(
            "resolved command identity belongs to another debugger transition",
        ));
    }
    validate_expected_receipt(operation.receipt(), expected_receipt, mode, exact_request)?;
    Ok(Some(operation))
}

/// Loads one exact accepted debugger operation independently of the current aggregate frontier.
///
/// The returned historical event/state comes from immutable event and state-history records. Its
/// `current_state` is reconstructed separately from a coherent current aggregate snapshot.
///
/// # Errors
/// Returns a typed integrity failure for a detached command receipt, malformed event/history,
/// invalid retained claim receipt, or divergent current aggregate.
pub fn load_debugger_operation(
    journal: &SqliteJournal,
    job_id: DebuggerJobId,
    command_id: CommandId,
) -> Result<Option<CommittedDebuggerOperation>, DebuggerError> {
    let Some(batch) = journal.command_batch(command_id).map_err(journal_error)? else {
        return Ok(None);
    };
    operation_from_batch(journal, job_id, batch).map(Some)
}

fn operation_from_batch(
    journal: &SqliteJournal,
    job_id: DebuggerJobId,
    batch: CommittedBatch,
) -> Result<CommittedDebuggerOperation, DebuggerError> {
    let aggregate = debugger_aggregate_key(job_id)?;
    let [record] = batch.records() else {
        return Err(recovery("debugger operation command batch is not exactly one event"));
    };
    if record.aggregate() != aggregate || record.command_id() != batch.command_id() {
        return Err(recovery("debugger operation event belongs to another aggregate or command"));
    }
    let sequence = record.sequence().get();
    let state_key = debugger_state_key(job_id);
    let prior = if sequence == 1 {
        None
    } else {
        Some(load_historical_state(journal, &state_key, sequence - 1)?)
    };
    let frame = decode_message::<DebuggerEventFrame>(
        record.frame_bytes(),
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    let event = frame.check(prior.as_ref())?;
    let historical_state = apply_event(prior.as_ref(), &event)?;
    let stored_historical = journal
        .state_record_revision(DEBUGGER_STATE_NAMESPACE, &state_key, sequence)
        .map_err(journal_error)?
        .ok_or_else(|| recovery("debugger operation has no historical successor checkpoint"))?;
    let stored_frame = decode_message::<DebuggerStateFrame>(
        stored_historical.bytes(),
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    if event.job_id() != job_id
        || event.sequence() != sequence
        || event.id() != record.event_id()
        || event.command_id() != record.command_id()
        || event.previous_event() != record.previous_event_id()
        || peritus_evidence::revision_digest(historical_state.revision())
            != record.revision_digest()
        || stored_historical.producing_position() != batch.last_position()
        || stored_historical.revision() != sequence
        || !stored_frame.matches_state(&historical_state)
    {
        return Err(recovery(
            "debugger operation event and historical successor checkpoint differ",
        ));
    }
    let receipt_record = journal
        .state_record(DEBUGGER_RECEIPT_NAMESPACE, &receipt_key(batch.command_id()))
        .map_err(journal_error)?;
    let receipt = match receipt_record {
        Some(record) => {
            if record.revision() != 1 || record.producing_position() != batch.last_position() {
                return Err(recovery(
                    "debugger operation receipt was not installed with its command batch",
                ));
            }
            let receipt = DebuggerOperationReceipt::decode(record.bytes())?;
            validate_retained_receipt(&receipt, &batch, record.digest(), &event, &historical_state)?;
            receipt
        }
        None => DebuggerOperationReceipt::legacy(
            batch.command_id(),
            event.id(),
            job_id,
            sequence,
            event.command_digest(),
            batch.request_digest(),
            record.frame_digest(),
            historical_state.state_digest(),
        ),
    };
    let current_state = load_debugger_replay(journal, job_id)?
        .rebuild()?
        .ok_or_else(|| recovery("accepted debugger operation has no current aggregate"))?;
    if current_state.sequence() < historical_state.sequence()
        || (current_state.sequence() == historical_state.sequence()
            && current_state != historical_state)
    {
        return Err(recovery(
            "current debugger state is behind or differs from the historical operation",
        ));
    }
    Ok(CommittedDebuggerOperation::new(
        batch,
        receipt,
        event,
        historical_state,
        current_state,
    ))
}

fn load_historical_state(
    journal: &SqliteJournal,
    state_key: &[u8],
    revision: u64,
) -> Result<DebuggerState, DebuggerError> {
    let record = journal
        .state_record_revision(DEBUGGER_STATE_NAMESPACE, state_key, revision)
        .map_err(journal_error)?
        .ok_or_else(|| recovery("debugger operation predecessor checkpoint is missing"))?;
    if record.revision() != revision {
        return Err(recovery("debugger historical checkpoint revision differs"));
    }
    decode_message::<DebuggerStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
        .map(DebuggerStateFrame::into_state)
        .map_err(codec)
}

fn validate_retained_receipt(
    receipt: &DebuggerOperationReceipt,
    batch: &CommittedBatch,
    stored_digest: peritus_types::Sha256Digest,
    event: &DebuggerEvent,
    state: &DebuggerState,
) -> Result<(), DebuggerError> {
    if peritus_codec::sha256(&receipt.canonical_bytes()?) != stored_digest
        || receipt.command_id() != batch.command_id()
        || receipt.event_id() != event.id()
        || receipt.job_id() != event.job_id()
        || receipt.sequence() != event.sequence()
        || receipt.command_digest() != event.command_digest()
        || receipt.request_digest() != batch.request_digest()
        || receipt.event_frame_digest() != batch.records()[0].frame_digest()
        || receipt.successor_state_digest() != state.state_digest()
    {
        return Err(recovery(
            "retained debugger operation receipt differs from immutable history",
        ));
    }
    Ok(())
}

fn validate_expected_receipt(
    observed: DebuggerOperationReceipt,
    expected: DebuggerOperationReceipt,
    mode: CommitMode,
    exact_request: bool,
) -> Result<(), DebuggerError> {
    if observed.mode() == DebuggerCommitMode::Legacy {
        return if exact_request || mode.is_claim_bound() {
            Ok(())
        } else {
            Err(conflict("legacy debugger operation request digest differs"))
        };
    }
    if observed.command_id() != expected.command_id()
        || observed.event_id() != expected.event_id()
        || observed.job_id() != expected.job_id()
        || observed.sequence() != expected.sequence()
        || observed.command_digest() != expected.command_digest()
        || observed.base_request_digest() != expected.base_request_digest()
        || observed.event_frame_digest() != expected.event_frame_digest()
        || observed.successor_state_digest() != expected.successor_state_digest()
        || observed.mode() != mode.receipt_mode()
    {
        return Err(conflict("resolved debugger operation receipt differs from the retry"));
    }
    if exact_request {
        if observed != expected {
            return Err(conflict("exact debugger request has a different retained receipt"));
        }
        return Ok(());
    }
    let (Some(original), Some(replacement)) =
        (observed.original_claim(), expected.original_claim())
    else {
        return Err(conflict("claim-bound debugger retry has no retained claim identity"));
    };
    if original.outbox_id() != replacement.outbox_id()
        || replacement.fence() <= original.fence()
        || observed.request_digest() == expected.request_digest()
    {
        return Err(conflict(
            "replacement debugger claim does not advance the original retained fence",
        ));
    }
    Ok(())
}

fn operation_receipt(
    command: &DebuggerCommand,
    event: &DebuggerEvent,
    event_frame_digest: peritus_types::Sha256Digest,
    base_request_digest: peritus_types::Sha256Digest,
    request_digest: peritus_types::Sha256Digest,
    mode: CommitMode,
) -> Result<DebuggerOperationReceipt, DebuggerError> {
    let original_claim = mode
        .claim()
        .map(|claim| claim.id().map(|id| claim_receipt(id, claim.fence())))
        .transpose()?;
    DebuggerOperationReceipt::retained(
        command.command_id(),
        event.id(),
        command.job_id(),
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

fn conflict(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::IdempotencyConflict,
        DebuggerOperation::CommitTransition,
        DebuggerRecovery::Quarantine,
        detail,
    )
}

fn codec(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Corruption,
        DebuggerOperation::DecodeProtocol,
        DebuggerRecovery::Quarantine,
        error.to_string(),
    )
}

fn journal_error(error: impl core::fmt::Display) -> DebuggerError {
    binding::journal(error)
}

fn recovery(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Recovery,
        DebuggerOperation::Recover,
        DebuggerRecovery::Quarantine,
        detail,
    )
}
