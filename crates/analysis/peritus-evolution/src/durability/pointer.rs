//! Atomic ordinary production-pointer event and checkpoint persistence.

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_journal::{
    AppendRequest, ArtifactDependency, CommandResolution, CommittedBatch, EventDraft, ExactFrame,
    HeadExpectation, SqliteJournal, StateInstall,
};
use peritus_types::EventSequence;

use crate::{
    EvolutionError, PointerCommand, PointerCommandKind, PointerTransition, ProductionHarnessState,
    wire::{PointerCommandFrame, PointerEventFrame, PointerStateFrame},
};

use super::{
    POINTER_STATE_NAMESPACE, binding, campaign::codec, campaign::journal_error, campaign::recovery,
    directive::pointer_outbox, pointer_aggregate_key, pointer_state_key,
};

/// Atomically appends one accepted pointer event and its complete checkpoint.
///
/// # Errors
/// Rejects transition drift, stale C0 fences, missing artifacts, protocol errors, or journal
/// failures.
pub fn commit_pointer_transition(
    journal: &mut SqliteJournal,
    command: &PointerCommand,
    transition: &PointerTransition,
) -> Result<CommittedBatch, EvolutionError> {
    binding::validate_pointer(command, transition)?;
    let aggregate = pointer_aggregate_key(command.project_id())?;
    let state_key = pointer_state_key(command.project_id());
    let command_bytes = encode_message(
        &PointerCommandFrame::from_command(command).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    let event_bytes = encode_message(
        &PointerEventFrame::from_event(transition.event()).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    let state_bytes = encode_message(
        &PointerStateFrame::from_state(transition.state()).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    let request_digest = peritus_codec::sha256(&command_bytes);
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
    let head = journal.head(aggregate).map_err(journal_error)?;
    let current =
        journal.state_record(POINTER_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    validate_current(command, head, current.as_ref())?;
    let event = transition.event();
    let draft = EventDraft::new(
        aggregate,
        EventSequence::new(event.sequence()).map_err(|_| binding::binding("zero pointer event"))?,
        event.id(),
        event.previous_event(),
        ExactFrame::new(event_bytes).map_err(journal_error)?,
        transition.state().state_digest(),
        Vec::new(),
    )
    .map_err(journal_error)?;
    let install = StateInstall::new(
        POINTER_STATE_NAMESPACE,
        state_key,
        current.as_ref().map(peritus_journal::DurableStateRecord::revision),
        transition.state().sequence(),
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
        artifact_dependencies(command.kind()),
        None,
        None,
        pointer_outbox(command, transition.state())?,
    );
    journal.append(request.plan().map_err(journal_error)?).map_err(journal_error)
}

pub(super) fn artifact_dependencies(kind: &PointerCommandKind) -> Vec<ArtifactDependency> {
    let mut values = match kind {
        PointerCommandKind::InitializeProductionHarness { evidence_artifact, .. } => {
            vec![ArtifactDependency::new(*evidence_artifact)]
        }
        PointerCommandKind::PreparePromotion(value) => {
            vec![ArtifactDependency::new(value.evidence_bundle_artifact())]
        }
        PointerCommandKind::PrepareRollback(value) => {
            vec![ArtifactDependency::new(value.evidence_bundle_artifact())]
        }
        PointerCommandKind::ActivatePromotion { .. }
        | PointerCommandKind::ActivateRollback { .. }
        | PointerCommandKind::CancelPending { .. } => Vec::new(),
    };
    values.sort_unstable();
    values.dedup();
    values
}

pub(super) fn validate_current(
    command: &PointerCommand,
    head: Option<peritus_journal::AggregateHead>,
    current: Option<&peritus_journal::DurableStateRecord>,
) -> Result<(), EvolutionError> {
    if head.is_some() != current.is_some() {
        return Err(recovery("pointer head/checkpoint presence differs"));
    }
    match head {
        None if command.expected_sequence() != 0 => {
            return Err(binding::binding("pointer genesis expects an existing head"));
        }
        Some(observed)
            if observed.sequence().get() != command.expected_sequence()
                || Some(observed.event_id()) != command.expected_head() =>
        {
            return Err(binding::binding("pointer command fence differs from C0 head"));
        }
        _ => {}
    }
    if let Some(record) = current {
        if record.revision() != command.expected_sequence() {
            return Err(recovery("pointer checkpoint revision differs from C0 head"));
        }
        let frame = decode_message::<PointerStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
            .map_err(codec)?;
        if frame.project_id() != command.project_id()
            || frame.sequence() != command.expected_sequence()
            || Some(frame.last_event_id()) != command.expected_head()
            || frame.state_digest() != command.prior_state_digest()
        {
            return Err(binding::binding("pointer command fence differs from durable checkpoint"));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn resolve_existing(
    journal: &SqliteJournal,
    command: &PointerCommand,
    aggregate: peritus_journal::AggregateKey,
    state_key: &[u8],
    event_bytes: &[u8],
    state: &ProductionHarnessState,
    request_digest: peritus_types::Sha256Digest,
) -> Result<Option<CommittedBatch>, EvolutionError> {
    let batch = match journal
        .resolve_command(command.command_id(), request_digest)
        .map_err(journal_error)?
    {
        CommandResolution::Committed(batch) => batch,
        CommandResolution::Conflict { .. } => {
            return Err(binding::binding("pointer command identity has another request"));
        }
        CommandResolution::DefinitelyAbsent => return Ok(None),
    };
    let checkpoint = journal
        .state_record(POINTER_STATE_NAMESPACE, state_key)
        .map_err(journal_error)?
        .ok_or_else(|| recovery("resolved pointer command has no checkpoint"))?;
    if batch.records().len() != 1
        || batch.records()[0].aggregate() != aggregate
        || batch.records()[0].frame_bytes() != event_bytes
    {
        return Err(recovery("resolved pointer command differs from its event"));
    }
    let observed = decode_message::<PointerStateFrame>(checkpoint.bytes(), CodecLimits::PRODUCTION)
        .map_err(codec)?;
    if checkpoint.revision() == state.sequence() && observed.matches_state(state) {
        Ok(Some(batch))
    } else {
        Err(recovery("resolved pointer checkpoint differs from successor"))
    }
}
