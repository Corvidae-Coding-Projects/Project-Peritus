//! Atomic family-80 event, family-81 checkpoint, artifact, and outbox persistence.

use std::collections::BTreeSet;

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_journal::{
    AppendRequest, ArtifactDependency, CommandResolution, CommittedBatch, EventDraft, ExactFrame,
    HeadExpectation, OutboxAcknowledgement, OutboxDraft, OutboxId, OutboxMessage, OutboxState,
    SqliteJournal, StateInstall,
};
use peritus_types::{EventSequence, Sha256Digest};

use crate::{
    aggregate::{
        HarnessCommand, HarnessCommandKind, HarnessEventKind, HarnessState, HarnessTransition,
        ReconciliationDecision,
    },
    materialization::MaterializationPlanId,
    wire::{HarnessCommandFrame, HarnessEventFrame, HarnessStateFrame},
};

use super::{
    DurabilityError, DurabilityErrorKind, DurabilityRecovery, HARNESS_STATE_NAMESPACE, binding,
    harness_aggregate_key, harness_state_key,
};

const OUTBOX_DESTINATION: &str = "peritus.harness.materialize.v1";
const OUTBOX_MAX_ATTEMPTS: u16 = 16;

/// Exact claimed materialization directive to settle with a success/failure transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectiveClaim {
    plan_id: MaterializationPlanId,
    fence: u64,
}

impl DirectiveClaim {
    /// Validates one claimed C0 row against the complete exact plan directive.
    ///
    /// # Errors
    /// Rejects a non-claimed row, another outbox identity/destination/payload, or absent fence.
    pub fn from_message(
        plan: &crate::materialization::MaterializationPlan,
        message: &OutboxMessage,
    ) -> Result<Self, DurabilityError> {
        let expected_payload = plan.canonical_bytes().map_err(nested)?;
        if message.id() != outbox_id(plan.id())?
            || message.state() != OutboxState::Claimed
            || message.destination() != OUTBOX_DESTINATION
            || message.payload() != expected_payload
        {
            return Err(binding::binding("C0 outbox claim differs from the exact plan directive"));
        }
        let fence =
            message.fence().ok_or_else(|| binding::binding("claimed directive has no fence"))?;
        Ok(Self { plan_id: plan.id(), fence })
    }
    /// Returns the claimed plan identity.
    #[must_use]
    pub const fn plan_id(self) -> MaterializationPlanId {
        self.plan_id
    }
    /// Returns the exact C0 claim fence.
    #[must_use]
    pub const fn fence(self) -> u64 {
        self.fence
    }
}

/// Atomically appends a non-settlement transition and installs its complete checkpoint.
///
/// Planning also records all bound finalized artifacts and creates one stable outbox directive.
/// Success/failure settlement must use [`commit_harness_settlement`] so C0 can acknowledge the
/// exact claimed directive in the same transaction.
///
/// # Errors
/// Rejects cross-record mismatch, stale CAS, limits, command conflict, or settlement without a
/// claimed directive.
pub fn commit_harness_transition(
    journal: &mut SqliteJournal,
    command: &HarnessCommand,
    transition: &HarnessTransition,
) -> Result<CommittedBatch, DurabilityError> {
    commit(journal, command, transition, None)
}

/// Atomically appends success/failure reconciliation and acknowledges its claimed directive.
///
/// # Errors
/// Rejects an unrelated claim, a non-settlement command, a stale fence, or any ordinary commit
/// integrity failure.
pub fn commit_harness_settlement(
    journal: &mut SqliteJournal,
    command: &HarnessCommand,
    transition: &HarnessTransition,
    claim: DirectiveClaim,
) -> Result<CommittedBatch, DurabilityError> {
    commit(journal, command, transition, Some(claim))
}

fn commit(
    journal: &mut SqliteJournal,
    command: &HarnessCommand,
    transition: &HarnessTransition,
    claim: Option<DirectiveClaim>,
) -> Result<CommittedBatch, DurabilityError> {
    binding::validate(command, transition)?;
    validate_claim(command.kind(), claim)?;
    let event = transition.event();
    let state = transition.state();
    let aggregate = harness_aggregate_key(command.harness_id())?;
    let state_key = harness_state_key(command.harness_id());
    let command_bytes = encode_message(
        &HarnessCommandFrame::from_command(command).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    let event_bytes = encode_message(
        &HarnessEventFrame::from_event(event).map_err(codec)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(codec)?;
    let state_bytes =
        encode_message(&HarnessStateFrame::from_state(state), CodecLimits::PRODUCTION)
            .map_err(codec)?;
    validate_lengths(&command_bytes, &event_bytes, &state_bytes, state)?;
    let base_digest = peritus_codec::sha256(&command_bytes);
    let request_digest = claim.map_or(base_digest, |value| acknowledged_digest(base_digest, value));
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
        journal.state_record(HARNESS_STATE_NAMESPACE, &state_key).map_err(journal_error)?;
    validate_current(command, head, current.as_ref())?;
    let draft = EventDraft::new(
        aggregate,
        EventSequence::new(event.sequence())
            .map_err(|_| binding::binding("event sequence is zero"))?,
        event.id(),
        event.previous_event(),
        ExactFrame::new(event_bytes).map_err(journal_error)?,
        event.revision_digest().digest(),
        Vec::new(),
    )
    .map_err(journal_error)?;
    let install = StateInstall::new(
        HARNESS_STATE_NAMESPACE,
        state_key,
        current.as_ref().map(peritus_journal::DurableStateRecord::revision),
        state.sequence(),
        state_bytes,
    )
    .map_err(journal_error)?;
    let expectation = head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
    let outbox = planning_outbox(command.kind())?;
    let dependencies = transition_dependencies(event.kind(), event.artifact_roots());
    let request = AppendRequest::new(
        journal.store_id(),
        command.command_id(),
        base_digest,
        vec![expectation],
        vec![draft],
        vec![install],
        dependencies,
        None,
        None,
        outbox,
    );
    let request = if let Some(value) = claim {
        request
            .with_outbox_acknowledgements(vec![outbox_acknowledgement(value)?])
            .map_err(journal_error)?
    } else {
        request
    };
    journal.append(request.plan().map_err(journal_error)?).map_err(journal_error)
}

fn validate_current(
    command: &HarnessCommand,
    head: Option<peritus_journal::AggregateHead>,
    current: Option<&peritus_journal::DurableStateRecord>,
) -> Result<(), DurabilityError> {
    if head.is_some() != current.is_some() {
        return Err(recovery("harness journal head/checkpoint presence differs"));
    }
    match head {
        None if command.expected_sequence() != 0 => {
            return Err(binding::binding("genesis expects an existing C0 head"));
        }
        Some(observed)
            if observed.sequence().get() != command.expected_sequence()
                || Some(observed.event_id()) != command.expected_previous_event() =>
        {
            return Err(binding::binding("command fence differs from C0 head"));
        }
        _ => {}
    }
    if current.is_some_and(|record| record.revision() != command.expected_sequence()) {
        return Err(recovery("harness checkpoint revision differs from C0 head"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, reason = "exact idempotency evidence remains explicit")]
fn resolve_existing(
    journal_store: &SqliteJournal,
    command: &HarnessCommand,
    aggregate: peritus_journal::AggregateKey,
    state_key: &[u8],
    event_bytes: &[u8],
    state: &HarnessState,
    request_digest: Sha256Digest,
) -> Result<Option<CommittedBatch>, DurabilityError> {
    let batch = match journal_store
        .resolve_command(command.command_id(), request_digest)
        .map_err(journal_error)?
    {
        CommandResolution::Committed(batch) => batch,
        CommandResolution::Conflict { .. } => {
            return Err(DurabilityError::new(
                DurabilityErrorKind::IdempotencyConflict,
                DurabilityRecovery::Quarantine,
                "command identity was committed with another canonical request digest",
            ));
        }
        CommandResolution::DefinitelyAbsent => return Ok(None),
    };
    let checkpoint = journal_store
        .state_record(HARNESS_STATE_NAMESPACE, state_key)
        .map_err(journal_error)?
        .ok_or_else(|| recovery("resolved command has no harness checkpoint"))?;
    if batch.records().len() != 1
        || batch.records()[0].frame_bytes() != event_bytes
        || batch.records()[0].aggregate() != aggregate
    {
        return Err(recovery("resolved command differs from expected harness event"));
    }
    let observed = decode_message::<HarnessStateFrame>(checkpoint.bytes(), CodecLimits::PRODUCTION)
        .map_err(codec)?;
    if checkpoint.revision() == state.sequence() && observed.matches_state(state) {
        return Ok(Some(batch));
    }
    if observed.harness_id() == state.harness_id() && observed.sequence() > state.sequence() {
        return Err(DurabilityError::new(
            DurabilityErrorKind::Recovery,
            DurabilityRecovery::ReplayAggregate,
            "resolved harness aggregate advanced; replay required",
        ));
    }
    Err(recovery("resolved harness checkpoint differs from exact successor"))
}

fn planning_outbox(kind: &HarnessCommandKind) -> Result<Vec<OutboxDraft>, DurabilityError> {
    let HarnessCommandKind::PlanMaterialization { plan } = kind else {
        return Ok(Vec::new());
    };
    Ok(vec![
        OutboxDraft::new(
            outbox_id(plan.id())?,
            OUTBOX_DESTINATION.to_owned(),
            plan.canonical_bytes().map_err(nested)?,
            OUTBOX_MAX_ATTEMPTS,
        )
        .map_err(journal_error)?,
    ])
}

fn transition_dependencies(
    kind: &HarnessEventKind,
    revision_roots: &[Sha256Digest],
) -> Vec<ArtifactDependency> {
    let roots = match kind {
        HarnessEventKind::GenesisRegistered { .. }
        | HarnessEventKind::SuccessorRegistered { .. }
        | HarnessEventKind::MaterializationPlanned { .. } => {
            revision_roots.iter().copied().collect::<BTreeSet<_>>()
        }
        HarnessEventKind::MaterializationRecorded { receipt } => {
            BTreeSet::from([receipt.workspace_manifest_artifact()])
        }
        HarnessEventKind::PendingMaterializationReconciled {
            decision: ReconciliationDecision::Completed(receipt),
            ..
        } => BTreeSet::from([receipt.workspace_manifest_artifact()]),
        _ => BTreeSet::new(),
    };
    roots.into_iter().map(ArtifactDependency::new).collect()
}

fn validate_claim(
    kind: &HarnessCommandKind,
    claim: Option<DirectiveClaim>,
) -> Result<(), DurabilityError> {
    let expected = settled_plan(kind);
    match (expected, claim) {
        (None, None) => Ok(()),
        (Some(expected), Some(observed)) if expected == observed.plan_id => Ok(()),
        (Some(_), None) => Err(binding::binding("settlement requires the exact claimed directive")),
        _ => Err(binding::binding("directive claim does not match a settlement command")),
    }
}

const fn settled_plan(kind: &HarnessCommandKind) -> Option<MaterializationPlanId> {
    match kind {
        HarnessCommandKind::RecordMaterialization { receipt } => Some(receipt.plan_id()),
        HarnessCommandKind::RecordMaterializationFailure { failure } => Some(failure.plan_id()),
        HarnessCommandKind::ReconcilePendingMaterialization { plan_id, decision }
            if !matches!(decision, ReconciliationDecision::Retry) =>
        {
            Some(*plan_id)
        }
        _ => None,
    }
}

fn outbox_id(plan_id: MaterializationPlanId) -> Result<OutboxId, DurabilityError> {
    OutboxId::new(*plan_id.as_bytes()).map_err(journal_error)
}

fn outbox_acknowledgement(claim: DirectiveClaim) -> Result<OutboxAcknowledgement, DurabilityError> {
    OutboxAcknowledgement::new(outbox_id(claim.plan_id)?, claim.fence).map_err(journal_error)
}

fn acknowledged_digest(base: Sha256Digest, claim: DirectiveClaim) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(96);
    bytes.extend_from_slice(b"PERITUS-C0-OUTBOX-ACKNOWLEDGEMENTS\0");
    bytes.extend_from_slice(base.as_bytes());
    bytes.extend_from_slice(&1_u64.to_be_bytes());
    bytes.extend_from_slice(claim.plan_id.as_bytes());
    bytes.extend_from_slice(&claim.fence.to_be_bytes());
    peritus_codec::sha256(&bytes)
}

fn validate_lengths(
    command: &[u8],
    event: &[u8],
    state: &[u8],
    value: &HarnessState,
) -> Result<(), DurabilityError> {
    let event_limit = value.limits().max_event_bytes();
    let state_limit = value.limits().max_state_bytes();
    if payload_len(command) > event_limit
        || payload_len(event) > event_limit
        || payload_len(state) > state_limit
    {
        return Err(DurabilityError::new(
            DurabilityErrorKind::LimitExceeded,
            DurabilityRecovery::CorrectInput,
            "harness command, event, or complete checkpoint exceeds configured bytes",
        ));
    }
    Ok(())
}

fn payload_len(bytes: &[u8]) -> u64 {
    u64::try_from(bytes.len().saturating_sub(peritus_codec::HEADER_LEN)).unwrap_or(u64::MAX)
}

fn codec(error: impl core::fmt::Display) -> DurabilityError {
    DurabilityError::new(
        DurabilityErrorKind::Codec,
        DurabilityRecovery::Quarantine,
        error.to_string(),
    )
}
fn nested(error: impl core::fmt::Display) -> DurabilityError {
    codec(error)
}
fn journal_error(error: impl core::fmt::Display) -> DurabilityError {
    DurabilityError::new(
        DurabilityErrorKind::Journal,
        DurabilityRecovery::ReplayAggregate,
        error.to_string(),
    )
}
fn recovery(detail: &'static str) -> DurabilityError {
    DurabilityError::new(DurabilityErrorKind::Recovery, DurabilityRecovery::Quarantine, detail)
}
