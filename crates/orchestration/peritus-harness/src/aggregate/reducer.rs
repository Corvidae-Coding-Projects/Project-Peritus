//! Pure checked command decision and event replay reducer.

mod settlement;

use std::collections::{BTreeMap, BTreeSet};

use peritus_types::Sha256Digest;

use crate::{
    domain::{HarnessHistory, RevisionDigest},
    materialization::{MaterializationPlanId, MaterializationReceipt},
};

use super::{
    AggregateError, AggregateErrorKind, AggregateRecovery, DeliveryState, HarnessCommand,
    HarnessCommandKind, HarnessEvent, HarnessEventKind, HarnessState, HarnessTransition,
    PendingMaterialization,
};
use settlement::{reconcile, retain_failure, settle_receipt, validate_plan};

/// Decides one command without mutating the prior state.
///
/// # Errors
/// Rejection returns no event and leaves caller-owned prior state unchanged.
pub fn decide(
    prior: Option<&HarnessState>,
    command: &HarnessCommand,
) -> Result<HarnessTransition, AggregateError> {
    validate_fence(prior, command)?;
    let revision_digest = command_revision(prior, command.kind())?;
    let sequence = command
        .expected_sequence()
        .checked_add(1)
        .ok_or_else(|| invalid("aggregate event sequence overflowed"))?;
    let (kind, mut state) = apply_command(prior, command, sequence)?;
    state.refresh_digest()?;
    let artifact_roots = revision_artifact_roots(&state, revision_digest)?;
    let event = HarnessEvent::new(
        command.event_id(),
        command.command_id(),
        command.harness_id(),
        sequence,
        command.expected_previous_event(),
        command.prior_state_digest(),
        command.digest(),
        state.state_digest(),
        revision_digest,
        artifact_roots,
        kind,
    );
    Ok(HarnessTransition::new(event, state))
}

/// Applies one already-decoded event during deterministic replay.
///
/// # Errors
/// Rejects any sequence, predecessor, digest, or semantic transition mismatch.
pub fn apply_event(
    prior: Option<&HarnessState>,
    event: &HarnessEvent,
) -> Result<HarnessState, AggregateError> {
    let (expected_sequence, expected_previous, expected_digest) =
        prior.map_or((1, None, Sha256Digest::new([0; 32])), |state| {
            (state.sequence().saturating_add(1), Some(state.last_event_id()), state.state_digest())
        });
    if event.sequence() != expected_sequence
        || event.previous_event() != expected_previous
        || event.prior_state_digest() != expected_digest
    {
        return Err(replay("event fence differs from reconstructed state"));
    }
    let command_kind = event_to_command(event.kind().clone());
    let command = HarnessCommand::new(
        event.command_id(),
        event.id(),
        event.harness_id(),
        event.sequence() - 1,
        event.previous_event(),
        event.prior_state_digest(),
        command_kind,
    )?;
    if command.digest() != event.command_digest() {
        return Err(replay("event command digest differs from semantic payload"));
    }
    let transition = decide(prior, &command)?;
    if transition.event() != event {
        return Err(replay("event successor differs from deterministic reduction"));
    }
    Ok(transition.into_parts().1)
}

#[allow(
    clippy::option_if_let_else,
    reason = "genesis and existing-state fences are clearer as two cases"
)]
fn validate_fence(
    prior: Option<&HarnessState>,
    command: &HarnessCommand,
) -> Result<(), AggregateError> {
    let valid = match prior {
        None => [
            command.expected_sequence() == 0,
            command.expected_previous_event().is_none(),
            command.prior_state_digest() == Sha256Digest::new([0; 32]),
        ]
        .into_iter()
        .all(core::convert::identity),
        Some(state) => [
            state.harness_id() == command.harness_id(),
            state.sequence() == command.expected_sequence(),
            Some(state.last_event_id()) == command.expected_previous_event(),
            state.state_digest() == command.prior_state_digest(),
        ]
        .into_iter()
        .all(core::convert::identity),
    };
    if valid {
        Ok(())
    } else {
        Err(AggregateError::new(
            AggregateErrorKind::StaleState,
            AggregateRecovery::ReplayAggregate,
            "command fence differs from exact aggregate state",
        ))
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "one exhaustive match keeps the closed command-to-event/state mapping auditable"
)]
fn apply_command(
    prior: Option<&HarnessState>,
    command: &HarnessCommand,
    sequence: u64,
) -> Result<(HarnessEventKind, HarnessState), AggregateError> {
    match (prior, command.kind()) {
        (None, HarnessCommandKind::RegisterGenesis { revision }) => {
            let history =
                HarnessHistory::new(revision.clone(), revision.graph().limits()).map_err(domain)?;
            let state = HarnessState {
                harness_id: revision.harness_id(),
                limits: history.limits(),
                sequence,
                last_event_id: command.event_id(),
                state_digest: Sha256Digest::new([0; 32]),
                history,
                pending: BTreeMap::new(),
                receipts: BTreeMap::new(),
                failures: Vec::new(),
                retired_receipts: BTreeSet::new(),
            };
            Ok((HarnessEventKind::GenesisRegistered { revision: revision.clone() }, state))
        }
        (None, _) => Err(invalid("the first command must register genesis")),
        (Some(_prior), HarnessCommandKind::RegisterGenesis { .. }) => {
            Err(invalid("genesis cannot be registered twice"))
        }
        (Some(prior), HarnessCommandKind::RegisterSuccessor { revision }) => {
            let mut state = prior.clone();
            state.history.append(revision.clone()).map_err(domain)?;
            advance(&mut state, command, sequence);
            Ok((HarnessEventKind::SuccessorRegistered { revision: revision.clone() }, state))
        }
        (Some(prior), HarnessCommandKind::PlanMaterialization { plan }) => {
            validate_plan(prior, plan)?;
            let mut state = prior.clone();
            state.pending.insert(plan.id(), PendingMaterialization::new(plan.clone()));
            advance(&mut state, command, sequence);
            Ok((HarnessEventKind::MaterializationPlanned { plan: plan.clone() }, state))
        }
        (
            Some(prior),
            HarnessCommandKind::AcknowledgeDirectiveDelivery { plan_id, delivered_at_millis },
        ) => {
            let mut state = prior.clone();
            let pending = state
                .pending
                .get_mut(plan_id)
                .ok_or_else(|| invalid("delivery acknowledgement names no pending plan"))?;
            if !matches!(pending.delivery, DeliveryState::Pending) {
                return Err(invalid("pending directive was already acknowledged"));
            }
            pending.delivery =
                DeliveryState::Acknowledged { delivered_at_millis: *delivered_at_millis };
            advance(&mut state, command, sequence);
            Ok((
                HarnessEventKind::DirectiveDeliveryAcknowledged {
                    plan_id: *plan_id,
                    delivered_at_millis: *delivered_at_millis,
                },
                state,
            ))
        }
        (Some(prior), HarnessCommandKind::RecordMaterialization { receipt }) => {
            let state = settle_receipt(prior, receipt, command, sequence)?;
            Ok((HarnessEventKind::MaterializationRecorded { receipt: receipt.clone() }, state))
        }
        (Some(prior), HarnessCommandKind::RecordMaterializationFailure { failure }) => {
            let mut state = prior.clone();
            let pending = state
                .pending
                .remove(&failure.plan_id())
                .ok_or_else(|| invalid("failure names no pending plan"))?;
            if pending.plan().digest() != failure.plan_digest() {
                return Err(conflict("failure plan digest differs from pending plan"));
            }
            retain_failure(&mut state, failure.clone())?;
            advance(&mut state, command, sequence);
            Ok((
                HarnessEventKind::MaterializationFailureRecorded { failure: failure.clone() },
                state,
            ))
        }
        (
            Some(prior),
            HarnessCommandKind::ReconcilePendingMaterialization { plan_id, decision },
        ) => {
            let state = reconcile(prior, *plan_id, decision, command, sequence)?;
            Ok((
                HarnessEventKind::PendingMaterializationReconciled {
                    plan_id: *plan_id,
                    decision: decision.clone(),
                },
                state,
            ))
        }
        (Some(prior), HarnessCommandKind::RetireSettledReceipt { receipt_id }) => {
            let mut state = prior.clone();
            if state.receipts.remove(receipt_id).is_none()
                || !state.retired_receipts.insert(*receipt_id)
            {
                return Err(invalid("receipt is absent or already retired"));
            }
            advance(&mut state, command, sequence);
            Ok((HarnessEventKind::SettledReceiptRetired { receipt_id: *receipt_id }, state))
        }
    }
}

fn command_revision(
    prior: Option<&HarnessState>,
    kind: &HarnessCommandKind,
) -> Result<RevisionDigest, AggregateError> {
    match kind {
        HarnessCommandKind::RegisterGenesis { revision }
        | HarnessCommandKind::RegisterSuccessor { revision } => Ok(revision.digest()),
        HarnessCommandKind::PlanMaterialization { plan } => Ok(plan.revision_digest()),
        HarnessCommandKind::RecordMaterialization { receipt } => Ok(receipt.revision_digest()),
        HarnessCommandKind::AcknowledgeDirectiveDelivery { plan_id, .. }
        | HarnessCommandKind::ReconcilePendingMaterialization { plan_id, .. } => {
            pending_revision(prior, *plan_id)
        }
        HarnessCommandKind::RecordMaterializationFailure { failure } => {
            pending_revision(prior, failure.plan_id())
        }
        HarnessCommandKind::RetireSettledReceipt { receipt_id } => prior
            .and_then(|state| state.receipt(*receipt_id))
            .map(MaterializationReceipt::revision_digest)
            .ok_or_else(|| invalid("command names no retained receipt revision")),
    }
}

fn pending_revision(
    prior: Option<&HarnessState>,
    plan_id: MaterializationPlanId,
) -> Result<RevisionDigest, AggregateError> {
    prior
        .and_then(|state| state.pending_plan(plan_id))
        .map(|pending| pending.plan().revision_digest())
        .ok_or_else(|| invalid("command names no pending plan revision"))
}

fn revision_artifact_roots(
    state: &HarnessState,
    revision_digest: RevisionDigest,
) -> Result<Vec<Sha256Digest>, AggregateError> {
    let revision = state
        .history()
        .revision(revision_digest)
        .ok_or_else(|| invalid("event revision is absent from successor history"))?;
    let mut roots = BTreeSet::new();
    for root in revision.artifact_roots() {
        roots.insert(root.content_digest());
        if let Some(executable) = root.executable_artifact_digest() {
            roots.insert(executable.digest());
        }
    }
    Ok(roots.into_iter().collect())
}

const fn advance(state: &mut HarnessState, command: &HarnessCommand, sequence: u64) {
    state.sequence = sequence;
    state.last_event_id = command.event_id();
}

fn event_to_command(kind: HarnessEventKind) -> HarnessCommandKind {
    match kind {
        HarnessEventKind::GenesisRegistered { revision } => {
            HarnessCommandKind::RegisterGenesis { revision }
        }
        HarnessEventKind::SuccessorRegistered { revision } => {
            HarnessCommandKind::RegisterSuccessor { revision }
        }
        HarnessEventKind::MaterializationPlanned { plan } => {
            HarnessCommandKind::PlanMaterialization { plan }
        }
        HarnessEventKind::DirectiveDeliveryAcknowledged { plan_id, delivered_at_millis } => {
            HarnessCommandKind::AcknowledgeDirectiveDelivery { plan_id, delivered_at_millis }
        }
        HarnessEventKind::MaterializationRecorded { receipt } => {
            HarnessCommandKind::RecordMaterialization { receipt }
        }
        HarnessEventKind::MaterializationFailureRecorded { failure } => {
            HarnessCommandKind::RecordMaterializationFailure { failure }
        }
        HarnessEventKind::PendingMaterializationReconciled { plan_id, decision } => {
            HarnessCommandKind::ReconcilePendingMaterialization { plan_id, decision }
        }
        HarnessEventKind::SettledReceiptRetired { receipt_id } => {
            HarnessCommandKind::RetireSettledReceipt { receipt_id }
        }
    }
}

fn domain(error: impl core::fmt::Display) -> AggregateError {
    AggregateError::new(
        AggregateErrorKind::Revision,
        AggregateRecovery::CorrectCommand,
        error.to_string(),
    )
}
fn invalid(detail: &'static str) -> AggregateError {
    AggregateError::new(
        AggregateErrorKind::InvalidCommand,
        AggregateRecovery::CorrectCommand,
        detail,
    )
}
fn conflict(detail: &'static str) -> AggregateError {
    AggregateError::new(AggregateErrorKind::Conflict, AggregateRecovery::Quarantine, detail)
}
fn limit(detail: &'static str) -> AggregateError {
    AggregateError::new(
        AggregateErrorKind::LimitExceeded,
        AggregateRecovery::CorrectCommand,
        detail,
    )
}
fn replay(detail: &'static str) -> AggregateError {
    AggregateError::new(AggregateErrorKind::Replay, AggregateRecovery::Quarantine, detail)
}
