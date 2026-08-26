//! Atomic family-83 event, family-84 checkpoint, artifact, and outbox persistence.

mod claim;
mod outbox;

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_journal::{
    AppendRequest, CommandResolution, CommittedBatch, EventDraft, ExactFrame, HeadExpectation,
    SqliteJournal, StateInstall,
};
use peritus_types::EventSequence;

use crate::{
    DebuggerCommand, DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery,
    DebuggerState, DebuggerTransition,
    wire::{DebuggerCommandFrame, DebuggerEventFrame, DebuggerStateFrame},
};

use super::{
    DEBUGGER_STATE_NAMESPACE, DebuggerDirectiveClaim, binding, debugger_aggregate_key,
    debugger_state_key,
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
) -> Result<CommittedBatch, DebuggerError> {
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
) -> Result<CommittedBatch, DebuggerError> {
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
) -> Result<CommittedBatch, DebuggerError> {
    commit(journal, command, transition, CommitMode::Settlement(claim.into()))
}

#[derive(Clone, Copy)]
pub(super) enum CommitMode {
    Ordinary,
    Claimed(DebuggerDirectiveClaim),
    Settlement(DebuggerDirectiveClaim),
}

fn commit(
    journal: &mut SqliteJournal,
    command: &DebuggerCommand,
    transition: &DebuggerTransition,
    mode: CommitMode,
) -> Result<CommittedBatch, DebuggerError> {
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
    let install = StateInstall::new(
        DEBUGGER_STATE_NAMESPACE,
        state_key,
        current.as_ref().map(peritus_journal::DurableStateRecord::revision),
        state.sequence(),
        state_bytes,
    )
    .map_err(journal_error)?;
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
        vec![install],
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
    journal.append(request.plan().map_err(journal_error)?).map_err(journal_error)
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
    journal_store: &SqliteJournal,
    command: &DebuggerCommand,
    aggregate: peritus_journal::AggregateKey,
    state_key: &[u8],
    event_bytes: &[u8],
    state: &DebuggerState,
    request_digest: peritus_types::Sha256Digest,
) -> Result<Option<CommittedBatch>, DebuggerError> {
    let batch = match journal_store
        .resolve_command(command.command_id(), request_digest)
        .map_err(journal_error)?
    {
        CommandResolution::Committed(batch) => batch,
        CommandResolution::Conflict { .. } => {
            return Err(DebuggerError::new(
                DebuggerErrorKind::IdempotencyConflict,
                DebuggerOperation::CommitTransition,
                DebuggerRecovery::Quarantine,
                "command identity was committed with another exact request digest",
            ));
        }
        CommandResolution::DefinitelyAbsent => return Ok(None),
    };
    let checkpoint = journal_store
        .state_record(DEBUGGER_STATE_NAMESPACE, state_key)
        .map_err(journal_error)?
        .ok_or_else(|| recovery("resolved command has no debugger checkpoint"))?;
    if batch.records().len() != 1
        || batch.records()[0].frame_bytes() != event_bytes
        || batch.records()[0].aggregate() != aggregate
    {
        return Err(recovery("resolved command differs from its exact debugger event"));
    }
    let observed =
        decode_message::<DebuggerStateFrame>(checkpoint.bytes(), CodecLimits::PRODUCTION)
            .map_err(codec)?;
    if checkpoint.revision() == state.sequence() && observed.matches_state(state) {
        return Ok(Some(batch));
    }
    if observed.job_id() == state.job_id() && observed.sequence() > state.sequence() {
        return Err(DebuggerError::new(
            DebuggerErrorKind::Recovery,
            DebuggerOperation::Recover,
            DebuggerRecovery::ReplayAggregate,
            "resolved debugger aggregate advanced; replay required",
        ));
    }
    Err(recovery("resolved debugger checkpoint differs from the exact successor"))
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
