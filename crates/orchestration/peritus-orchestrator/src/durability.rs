//! Atomic C0 persistence, outbox installation, and checked E0 replay loading.

mod binding;

use peritus_codec::{CodecLimits, decode_message, encode_message, sha256};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, ArtifactDependency, CommandResolution,
    CommittedBatch, EventDraft, ExactFrame, HeadExpectation, OutboxDraft, OutboxId, SqliteJournal,
    StateInstall,
};
use peritus_types::{EventId, RunId};
use sha2::{Digest, Sha256};

use crate::replay::OrchestratorReplay;
use crate::wire::{OrchestratorCommandFrame, OrchestratorEventFrame, OrchestratorStateFrame};
use crate::{
    OrchestratorCommand, OrchestratorEventKind, OrchestratorState, OrchestratorTransition,
};

/// Journal-owned E0 checkpoint namespace.
pub const ORCHESTRATOR_STATE_NAMESPACE: u16 = 0xE001;
const STATE_KEY_DOMAIN: &[u8] = b"peritus.orchestrator.state.v1\0";

/// Derives the dedicated C0 orchestrator aggregate key.
///
/// # Errors
///
/// Returns an error when the E0 run identity cannot be represented by C0.
pub fn orchestrator_aggregate_key(run_id: RunId) -> Result<AggregateKey, crate::OrchestratorError> {
    let id = AggregateId::new(*run_id.as_bytes())
        .map_err(|_| external("E0 run identity cannot be represented by C0"))?;
    Ok(AggregateKey::new(AggregateKind::Orchestrator, id))
}

/// Derives the stable run-scoped E0 checkpoint key.
#[must_use]
pub fn orchestrator_state_key(run_id: RunId) -> Vec<u8> {
    let mut key = Vec::with_capacity(STATE_KEY_DOMAIN.len() + 16);
    key.extend_from_slice(STATE_KEY_DOMAIN);
    key.extend_from_slice(run_id.as_bytes());
    key
}

/// Atomically commits one family-77 event, family-78 checkpoint, and first outbox publication.
///
/// # Errors
///
/// Returns an error when semantic binding, canonical encoding, journal fencing, or the atomic C0
/// append fails.
pub fn commit_orchestrator_transition(
    journal: &mut SqliteJournal,
    command: &OrchestratorCommand,
    transition: &OrchestratorTransition,
) -> Result<CommittedBatch, crate::OrchestratorError> {
    binding::validate_binding(command, transition)?;
    let event = transition.event();
    let state = transition.state();
    let aggregate = orchestrator_aggregate_key(command.run_id())?;
    let state_key = orchestrator_state_key(command.run_id());
    let command_bytes =
        encode_message(&OrchestratorCommandFrame::from_command(command), CodecLimits::PRODUCTION)
            .map_err(|_| codec("family-76 command encoding failed"))?;
    let event_bytes =
        encode_message(&OrchestratorEventFrame::from_event(event), CodecLimits::PRODUCTION)
            .map_err(|_| codec("family-77 event encoding failed"))?;
    let state_bytes =
        encode_message(&OrchestratorStateFrame::from_state(state), CodecLimits::PRODUCTION)
            .map_err(|_| codec("family-78 checkpoint encoding failed"))?;
    if payload_len(&event_bytes)? > state.limits().event_bytes()
        || payload_len(&state_bytes)? > state.limits().state_bytes()
    {
        return Err(integrity("E0 event or checkpoint exceeds its configured byte bound"));
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
    let head = journal.head(aggregate).map_err(|_| external("C0 head load failed"))?;
    let current = journal
        .state_record(ORCHESTRATOR_STATE_NAMESPACE, &state_key)
        .map_err(|_| external("C0 checkpoint load failed"))?;
    if head.is_some() != current.is_some() {
        return Err(integrity("E0 C0 head/checkpoint presence differs"));
    }
    match head {
        None if command.expected_sequence() != 0 => {
            return Err(integrity("E0 non-genesis command has no C0 head"));
        }
        Some(observed)
            if observed.sequence().get() != command.expected_sequence()
                || Some(observed.event_id()) != command.expected_previous_event() =>
        {
            return Err(integrity("E0 command fence differs from C0 head"));
        }
        _ => {}
    }
    if current.as_ref().is_some_and(|item| item.revision() != command.expected_sequence()) {
        return Err(integrity("E0 checkpoint revision differs from C0 head"));
    }
    let draft = EventDraft::new(
        aggregate,
        event.sequence(),
        event.id(),
        event.previous_event(),
        ExactFrame::new(event_bytes).map_err(|_| codec("family-77 frame is invalid"))?,
        revision_digest(&event.revision()),
        causal_parents(event.kind()),
    )
    .map_err(|_| external("C0 rejected the E0 event draft"))?;
    let install = StateInstall::new(
        ORCHESTRATOR_STATE_NAMESPACE,
        state_key,
        current.as_ref().map(peritus_journal::DurableStateRecord::revision),
        state.sequence().get(),
        state_bytes,
    )
    .map_err(|_| external("C0 rejected the E0 checkpoint install"))?;
    let expectation = head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
    let request = AppendRequest::new(
        journal.store_id(),
        command.command_id(),
        request_digest,
        vec![expectation],
        vec![draft],
        vec![install],
        artifact_dependencies(state),
        None,
        None,
        outbox_drafts(command, state, &command_bytes)?,
    );
    journal
        .append(request.plan().map_err(|_| external("C0 rejected the E0 append plan"))?)
        .map_err(|_| external("C0 failed the E0 append"))
}

/// Loads canonical E0 events and the exact installed checkpoint.
///
/// # Errors
///
/// Returns an error when journal records are absent, corrupt, noncanonical, or inconsistent with
/// the installed checkpoint.
pub fn load_orchestrator_replay(
    journal: &SqliteJournal,
    run_id: RunId,
) -> Result<OrchestratorReplay, crate::OrchestratorError> {
    let aggregate = orchestrator_aggregate_key(run_id)?;
    let state_key = orchestrator_state_key(run_id);
    let records = journal
        .records_for_aggregate(aggregate)
        .map_err(|_| external("C0 E0 record load failed"))?;
    let state_record = journal
        .state_record(ORCHESTRATOR_STATE_NAMESPACE, &state_key)
        .map_err(|_| external("C0 E0 checkpoint load failed"))?;
    if records.is_empty() != state_record.is_none() {
        return Err(integrity("E0 events/checkpoint presence differs"));
    }
    let mut events = Vec::with_capacity(records.len());
    for record in records {
        let event =
            decode_message::<OrchestratorEventFrame>(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(|_| codec("family-77 journal event is corrupt"))?
                .into_event();
        if event.run_id() != run_id
            || event.sequence() != record.sequence()
            || event.id() != record.event_id()
            || event.command_id() != record.command_id()
            || event.previous_event() != record.previous_event_id()
            || revision_digest(&event.revision()) != record.revision_digest()
        {
            return Err(integrity("decoded E0 event differs from its C0 record"));
        }
        events.push(event);
    }
    let checkpoint = state_record
        .as_ref()
        .map(|record| {
            decode_message::<OrchestratorStateFrame>(record.bytes(), CodecLimits::PRODUCTION)
                .map_err(|_| codec("family-78 checkpoint is corrupt"))
        })
        .transpose()?;
    if let Some(checkpoint) = &checkpoint {
        let last = events.last().ok_or_else(|| integrity("E0 checkpoint has no event"))?;
        let record = state_record.as_ref().ok_or_else(|| integrity("E0 checkpoint vanished"))?;
        if checkpoint.run_id() != run_id
            || checkpoint.sequence() != last.sequence()
            || checkpoint.last_event_id() != last.id()
            || checkpoint.revision() != last.revision()
            || checkpoint.state_digest() != last.successor_state_digest()
            || record.revision() != checkpoint.sequence().get()
        {
            return Err(integrity("E0 checkpoint differs from its C0 aggregate head"));
        }
    }
    Ok(OrchestratorReplay::from_parts(journal.store_id(), events, checkpoint))
}

#[allow(clippy::too_many_arguments, reason = "complete C0 idempotency match remains explicit")]
fn resolve_existing(
    journal: &SqliteJournal,
    command: &OrchestratorCommand,
    aggregate: AggregateKey,
    state_key: &[u8],
    event_bytes: &[u8],
    state: &OrchestratorState,
    request_digest: peritus_types::Sha256Digest,
) -> Result<Option<CommittedBatch>, crate::OrchestratorError> {
    let batch = match journal
        .resolve_command(command.command_id(), request_digest)
        .map_err(|_| external("C0 command resolution failed"))?
    {
        CommandResolution::Committed(batch) => batch,
        CommandResolution::Conflict { .. } => {
            return Err(conflict("E0 command identity conflicts"));
        }
        CommandResolution::DefinitelyAbsent => return Ok(None),
    };
    let checkpoint = journal
        .state_record(ORCHESTRATOR_STATE_NAMESPACE, state_key)
        .map_err(|_| external("C0 checkpoint resolution failed"))?
        .ok_or_else(|| integrity("resolved E0 command lacks checkpoint"))?;
    if batch.records().len() != 1
        || batch.records()[0].frame_bytes() != event_bytes
        || batch.records()[0].aggregate() != aggregate
    {
        return Err(integrity("resolved E0 command differs from exact event"));
    }
    let observed =
        decode_message::<OrchestratorStateFrame>(checkpoint.bytes(), CodecLimits::PRODUCTION)
            .map_err(|_| codec("resolved family-78 checkpoint is corrupt"))?;
    if checkpoint.revision() == state.sequence().get() && observed.matches_state(state) {
        Ok(Some(batch))
    } else if observed.run_id() == state.binding().run_id()
        && observed.sequence().get() > state.sequence().get()
    {
        Err(crate::OrchestratorError::new(
            crate::OrchestratorErrorKind::StaleState,
            crate::OrchestratorRecoveryAction::Replay,
            "resolved E0 command belongs to an advanced aggregate",
        ))
    } else {
        Err(integrity("resolved E0 checkpoint differs from exact successor"))
    }
}

fn outbox_drafts(
    command: &OrchestratorCommand,
    state: &OrchestratorState,
    payload: &[u8],
) -> Result<Vec<OutboxDraft>, crate::OrchestratorError> {
    let crate::OrchestratorCommandKind::PublishDirective { directive } = command.kind() else {
        return Ok(Vec::new());
    };
    if state.pending_directive().is_none_or(|current| current.deliveries() != 1) {
        return Ok(Vec::new());
    }
    let id = OutboxId::new(*directive.id().as_bytes())
        .map_err(|_| external("directive identity cannot be represented by C0 outbox"))?;
    let draft = OutboxDraft::new(
        id,
        directive.destination().outbox_destination().to_owned(),
        payload.to_vec(),
        directive.maximum_deliveries(),
    )
    .map_err(|_| external("C0 rejected the E0 outbox draft"))?;
    Ok(vec![draft])
}

fn artifact_dependencies(state: &OrchestratorState) -> Vec<ArtifactDependency> {
    state
        .current_candidate()
        .artifact_digest()
        .map_or_else(Vec::new, |digest| vec![ArtifactDependency::new(digest)])
}

fn causal_parents(kind: &OrchestratorEventKind) -> Vec<EventId> {
    let mut values = match kind {
        OrchestratorEventKind::HandoffActivated { activation } => vec![
            activation.scheduler_head().last_event_id(),
            activation.collaboration_head().last_event_id(),
        ],
        OrchestratorEventKind::WriterObserved { observation, .. } => {
            vec![observation.head().last_event_id()]
        }
        OrchestratorEventKind::GatesObserved { observation, .. } => {
            vec![observation.head().last_event_id()]
        }
        OrchestratorEventKind::ReviewObserved { observation, .. } => {
            vec![observation.head().last_event_id()]
        }
        OrchestratorEventKind::FixerObserved { completion } => {
            let mut items = vec![completion.observation().head().last_event_id()];
            if let Some(review) = completion.review_observation() {
                items.push(review.head().last_event_id());
            }
            items
        }
        OrchestratorEventKind::RoleInfrastructureObserved { scheduler, collaboration } => {
            vec![scheduler.head().last_event_id(), collaboration.head().last_event_id()]
        }
        OrchestratorEventKind::KernelAcceptanceObserved { observation } => {
            vec![observation.event_id()]
        }
        OrchestratorEventKind::CancellationReconciled { observation } => {
            observation.head().map_or_else(Vec::new, |head| vec![head.last_event_id()])
        }
        _ => Vec::new(),
    };
    values.sort_unstable();
    values.dedup();
    values
}

fn payload_len(bytes: &[u8]) -> Result<u64, crate::OrchestratorError> {
    u64::try_from(bytes.len().saturating_sub(peritus_codec::HEADER_LEN))
        .map_err(|_| integrity("canonical E0 frame length is unrepresentable"))
}
fn revision_digest(revision: &peritus_types::RevisionTuple) -> peritus_types::Sha256Digest {
    let mut hash = Sha256::new();
    hash.update(b"peritus.orchestrator.revision.v1\0");
    hash.update(revision.acceptance_spec_id().as_bytes());
    hash.update(revision.harness_id().as_bytes());
    hash.update(revision.workspace_id().as_bytes());
    hash.update(revision.workspace_generation().get().to_be_bytes());
    hash.update(revision.workspace_revision().get().to_be_bytes());
    hash.update(revision.policy_id().as_bytes());
    hash.update(revision.provider_profile_id().as_bytes());
    peritus_types::Sha256Digest::new(hash.finalize().into())
}
const fn integrity(detail: &'static str) -> crate::OrchestratorError {
    crate::OrchestratorError::new(
        crate::OrchestratorErrorKind::Integrity,
        crate::OrchestratorRecoveryAction::Quarantine,
        detail,
    )
}
const fn external(detail: &'static str) -> crate::OrchestratorError {
    crate::OrchestratorError::new(
        crate::OrchestratorErrorKind::External,
        crate::OrchestratorRecoveryAction::Replay,
        detail,
    )
}
const fn codec(detail: &'static str) -> crate::OrchestratorError {
    crate::OrchestratorError::new(
        crate::OrchestratorErrorKind::Codec,
        crate::OrchestratorRecoveryAction::Quarantine,
        detail,
    )
}
const fn conflict(detail: &'static str) -> crate::OrchestratorError {
    crate::OrchestratorError::new(
        crate::OrchestratorErrorKind::Conflict,
        crate::OrchestratorRecoveryAction::Quarantine,
        detail,
    )
}
