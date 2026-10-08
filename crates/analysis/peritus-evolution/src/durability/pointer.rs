//! Atomic ordinary production-pointer event and checkpoint persistence.

use peritus_codec::{CodecLimits, encode_message};
use peritus_journal::{
    AppendRequest, ArtifactDependency, CommandResolution, CommittedBatch, EventDraft, ExactFrame,
    HeadExpectation, SqliteJournal,
};
use peritus_types::{EventSequence, ProjectId, Sha256Digest};

use crate::{
    ActivationId, CompatibilityWitness, DurableActivationHistory, DurableActivationOrigin,
    EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
    EvolutionStorageLimits, PendingActivation, PointerCommand, PointerCommandKind,
    PointerTransition, ProductionHarnessState, RollbackProposal,
    wire::{PointerCommandFrame, PointerEventFrame, PointerStateFrame},
};

use super::{
    POINTER_STATE_NAMESPACE, binding, campaign::codec, campaign::journal_error, campaign::recovery,
    checkpoint, directive::pointer_outbox, pointer_aggregate_key, pointer_state_key,
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
    commit_pointer_transition_with_storage(
        journal,
        command,
        transition,
        EvolutionStorageLimits::default(),
    )
}

/// Atomically appends one pointer transition with caller-selected physical checkpoint paging.
///
/// # Errors
/// Rejects transition drift, stale C0 fences, missing artifacts, protocol errors, or journal
/// failures.
pub fn commit_pointer_transition_with_storage(
    journal: &mut SqliteJournal,
    command: &PointerCommand,
    transition: &PointerTransition,
    storage: EvolutionStorageLimits,
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
    validate_current(journal, command, head, current.as_ref())?;
    validate_durable_rollback(journal, command)?;
    let event = transition.event();
    let draft = EventDraft::new(
        aggregate,
        EventSequence::new(event.sequence()).map_err(|_| binding::binding("zero pointer event"))?,
        event.id(),
        event.previous_event(),
        ExactFrame::new(event_bytes).map_err(journal_error)?,
        pointer_event_revision_digest(transition.state()),
        Vec::new(),
    )
    .map_err(journal_error)?;
    let installs = checkpoint::pointer_installs(
        &state_key,
        current.as_ref().map(peritus_journal::DurableStateRecord::revision),
        transition.state(),
        storage,
    )?;
    let expectation = head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
    let request = AppendRequest::new(
        journal.store_id(),
        command.command_id(),
        request_digest,
        vec![expectation],
        vec![draft],
        installs,
        artifact_dependencies(command.kind()),
        None,
        None,
        pointer_outbox(command, transition.state())?,
    );
    journal.append(request.plan().map_err(journal_error)?).map_err(journal_error)
}

/// Resolves an exact pointer command receipt before reducer capacity or state fences are read.
///
/// # Errors
/// Rejects a command identity already bound to another request or a receipt for another aggregate.
pub fn resolve_pointer_receipt(
    journal: &SqliteJournal,
    command: &PointerCommand,
) -> Result<Option<CommittedBatch>, EvolutionError> {
    let aggregate = pointer_aggregate_key(command.project_id())?;
    let command_bytes = encode_message(
        &PointerCommandFrame::from_command(command).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    match journal
        .resolve_command(command.command_id(), peritus_codec::sha256(&command_bytes))
        .map_err(journal_error)?
    {
        CommandResolution::Committed(batch) => {
            validate_resolved_receipt(
                journal,
                command,
                aggregate,
                &pointer_state_key(command.project_id()),
                &batch,
            )?;
            Ok(Some(batch))
        }
        CommandResolution::Conflict { .. } => {
            Err(binding::binding("pointer command identity has another request"))
        }
        CommandResolution::DefinitelyAbsent => Ok(None),
    }
}

/// Reconstructs the complete immutable activation ledger for one project.
///
/// # Errors
/// Rejects malformed events, missing historical checkpoints, legacy histories that do not exactly
/// match the former eviction rule, or contradictory activation provenance.
pub fn recover_activation_history(
    journal: &SqliteJournal,
    project_id: ProjectId,
) -> Result<DurableActivationHistory, EvolutionError> {
    Ok(super::replay::recover_pointer(journal, project_id)?
        .activation_history()
        .clone())
}

/// Resolves one exact activation from immutable C0 history rather than the checkpoint cache.
///
/// # Errors
/// Rejects any malformed or contradictory pointer history. Absence is explicit.
pub fn resolve_activation_origin(
    journal: &SqliteJournal,
    project_id: ProjectId,
    activation_id: ActivationId,
) -> Result<Option<DurableActivationOrigin>, EvolutionError> {
    Ok(recover_activation_history(journal, project_id)?.origin(activation_id).cloned())
}

/// Constructs a compatibility-checked rollback proposal from a durable activation origin.
///
/// # Errors
/// Rejects an absent target, stale current state, policy drift, or incompatible evidence.
pub fn prepare_rollback_proposal(
    journal: &SqliteJournal,
    state: &ProductionHarnessState,
    target_activation: ActivationId,
    witness: CompatibilityWitness,
    evidence_bundle_artifact: Sha256Digest,
) -> Result<RollbackProposal, EvolutionError> {
    let origin = resolve_activation_origin(journal, state.project_id(), target_activation)?
        .ok_or_else(missing_origin)?;
    RollbackProposal::new_from_origin(state, &origin, witness, evidence_bundle_artifact)
}

pub(super) fn pointer_event_revision_digest(
    state: &ProductionHarnessState,
) -> peritus_types::Sha256Digest {
    state.history().last().map_or_else(
        || state.state_digest(),
        |record| peritus_evidence::revision_digest(&record.successor().revision()),
    )
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
        | PointerCommandKind::CancelPending { .. }
        | PointerCommandKind::ExpandScope { .. } => Vec::new(),
    };
    values.sort_unstable();
    values.dedup();
    values
}

pub(super) fn validate_current(
    journal: &SqliteJournal,
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
        let frame = checkpoint::decode_pointer(journal, record, command.project_id())?;
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

pub(super) fn validate_durable_rollback(
    journal: &SqliteJournal,
    command: &PointerCommand,
) -> Result<(), EvolutionError> {
    if !matches!(
        command.kind(),
        PointerCommandKind::PrepareRollback(_) | PointerCommandKind::ActivateRollback { .. }
    ) {
        return Ok(());
    }
    let replay = super::replay::recover_pointer(journal, command.project_id())?;
    validate_replay_fence(&replay, command)?;
    validate_replay_rollback(&replay, command)
}

pub(super) fn validate_atomic_pointer_history(
    journal: &SqliteJournal,
    command: &PointerCommand,
    approval_use_digest: Sha256Digest,
) -> Result<(), EvolutionError> {
    let replay = super::replay::recover_pointer(journal, command.project_id())?;
    validate_replay_fence(&replay, command)?;
    validate_replay_rollback(&replay, command)?;
    if replay.activation_history().contains_approval_use(approval_use_digest) {
        return Err(binding::binding(
            "atomic activation reuses authority retained by durable activation history",
        ));
    }
    Ok(())
}

fn validate_replay_fence(
    replay: &super::replay::PointerReplay,
    command: &PointerCommand,
) -> Result<(), EvolutionError> {
    let state = replay
        .state()
        .ok_or_else(|| binding::binding("pointer command has no durable predecessor"))?;
    if replay.store_id() == replay.activation_history().store_id()
        && state.project_id() == command.project_id()
        && state.sequence() == command.expected_sequence()
        && Some(state.last_event()) == command.expected_head()
        && state.generation() == command.expected_generation()
        && state.state_digest() == command.prior_state_digest()
        && state.policy().digest() == command.policy_digest()
    {
        Ok(())
    } else {
        Err(binding::binding(
            "pointer command fence differs from reconstructed durable history",
        ))
    }
}

fn validate_replay_rollback(
    replay: &super::replay::PointerReplay,
    command: &PointerCommand,
) -> Result<(), EvolutionError> {
    let proposal = match command.kind() {
        PointerCommandKind::PrepareRollback(proposal) => Some(proposal),
        PointerCommandKind::ActivateRollback { rollback_id, .. } => {
            match replay.state().and_then(|state| state.pending()) {
                Some(PendingActivation::Rollback(proposal)) if proposal.id() == *rollback_id => {
                    Some(proposal)
                }
                _ => None,
            }
        }
        _ => return Ok(()),
    }
    .ok_or_else(|| binding::binding("rollback command has no exact durable prepared action"))?;
    if replay.activation_history().validates_proposal(proposal) {
        Ok(())
    } else {
        Err(binding::binding(
            "rollback target has no exact immutable activation origin",
        ))
    }
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
    let observed = validate_resolved_receipt(journal, command, aggregate, state_key, &batch)?;
    if batch.records()[0].frame_bytes() != event_bytes {
        return Err(recovery("resolved pointer command differs from its event"));
    }
    if observed.matches_state(state) {
        Ok(Some(batch))
    } else {
        Err(recovery("resolved pointer checkpoint differs from successor"))
    }
}

fn validate_resolved_receipt(
    journal: &SqliteJournal,
    command: &PointerCommand,
    aggregate: peritus_journal::AggregateKey,
    state_key: &[u8],
    batch: &CommittedBatch,
) -> Result<PointerStateFrame, EvolutionError> {
    let successor_sequence = command
        .expected_sequence()
        .checked_add(1)
        .ok_or_else(|| recovery("resolved pointer sequence overflows"))?;
    let [record] = batch.records() else {
        return Err(recovery("pointer receipt belongs to another durable effect"));
    };
    if record.aggregate() != aggregate
        || record.sequence().get() != successor_sequence
        || record.event_id() != command.event_id()
        || record.previous_event_id() != command.expected_head()
    {
        return Err(recovery("resolved pointer command differs from its event identity"));
    }
    let checkpoint = journal
        .state_record_revision(POINTER_STATE_NAMESPACE, state_key, successor_sequence)
        .map_err(journal_error)?
        .ok_or_else(|| recovery("resolved pointer command has no historical checkpoint"))?;
    let observed = checkpoint::decode_pointer(journal, &checkpoint, command.project_id())?;
    if checkpoint.producing_position() != batch.last_position()
        || checkpoint.revision() != successor_sequence
        || observed.project_id() != command.project_id()
        || observed.sequence() != successor_sequence
        || observed.last_event_id() != command.event_id()
    {
        return Err(recovery(
            "resolved pointer event and historical checkpoint have different origins",
        ));
    }
    Ok(observed)
}

const fn missing_origin() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::PolicyRejected,
        EvolutionOperation::Rollback,
        EvolutionRecovery::ObtainEvidence,
        "rollback target is absent from immutable activation history",
    )
}
