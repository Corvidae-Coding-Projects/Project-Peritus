//! Deterministic event replay and pending-work recovery classification.

use crate::{
    aggregate::{
        AggregateError, AggregateErrorKind, AggregateRecovery, DeliveryState, HarnessEvent,
        HarnessState, ReconciliationDecision, apply_event,
    },
    materialization::{
        MaterializationFailure, MaterializationPlan, MaterializationPlanId, MaterializationReceipt,
        WorkspaceSnapshot,
    },
};

/// Read-only recovery directive reconstructed from one durable pending plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingRecovery {
    plan: MaterializationPlan,
    delivery: DeliveryState,
}

impl PendingRecovery {
    /// Returns the exact committed plan eligible for idempotent redelivery or reconciliation.
    #[must_use]
    pub const fn plan(&self) -> &MaterializationPlan {
        &self.plan
    }
    /// Returns the last durable delivery observation.
    #[must_use]
    pub const fn delivery(&self) -> DeliveryState {
        self.delivery
    }
}

/// Exact C1 evidence used to classify one pending plan after recovery.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "closed restart evidence remains directly inspectable and is not stored in collections"
)]
pub enum RecoveryObservation {
    /// C1 is still at the plan's exact immutable pre-mutation snapshot.
    Untouched(WorkspaceSnapshot),
    /// A complete retained receipt proves the exact candidate already completed.
    Completed(MaterializationReceipt),
    /// Exact observations conflict with the plan and cannot be guessed into success.
    Conflict(MaterializationFailure),
}

/// Rebuilds complete state from genesis through a contiguous event sequence.
///
/// # Errors
/// Rejects empty input, orphan successors, stale fences, semantic/digest disagreement, impossible
/// materialization ordering, bounds, or terminal divergence.
pub fn replay(events: &[HarnessEvent]) -> Result<HarnessState, AggregateError> {
    let mut state = None;
    for event in events {
        state = Some(apply_event(state.as_ref(), event)?);
    }
    state.ok_or_else(|| {
        AggregateError::new(
            AggregateErrorKind::Replay,
            AggregateRecovery::ReplayAggregate,
            "cannot rebuild a harness aggregate from an empty event sequence",
        )
    })
}

/// Returns all pending directives in canonical plan-identity order.
#[must_use]
pub fn pending_recovery(state: &HarnessState) -> Vec<PendingRecovery> {
    state
        .pending()
        .values()
        .map(|pending| PendingRecovery {
            plan: pending.plan().clone(),
            delivery: pending.delivery(),
        })
        .collect()
}

/// Classifies exact restart evidence without performing a C1 effect or guessing success.
///
/// # Errors
/// Rejects observations for an absent plan, another snapshot, or another plan payload.
pub fn classify_recovery(
    state: &HarnessState,
    plan_id: MaterializationPlanId,
    observation: RecoveryObservation,
) -> Result<ReconciliationDecision, AggregateError> {
    let pending = state
        .pending_plan(plan_id)
        .ok_or_else(|| invalid("recovery observation names no pending materialization"))?;
    match observation {
        RecoveryObservation::Untouched(snapshot) if &snapshot == pending.plan().target() => {
            Ok(ReconciliationDecision::Retry)
        }
        RecoveryObservation::Completed(receipt)
            if receipt.plan_id() == plan_id
                && receipt.plan_digest() == pending.plan().digest()
                && receipt.revision_digest() == pending.plan().revision_digest()
                && receipt.before() == pending.plan().target() =>
        {
            Ok(ReconciliationDecision::Completed(receipt))
        }
        RecoveryObservation::Conflict(failure)
            if failure.plan_id() == plan_id && failure.plan_digest() == pending.plan().digest() =>
        {
            Ok(ReconciliationDecision::Conflict(failure))
        }
        _ => Err(invalid("recovery observation conflicts with the exact committed pending plan")),
    }
}

fn invalid(detail: &'static str) -> AggregateError {
    AggregateError::new(AggregateErrorKind::Replay, AggregateRecovery::Reconcile, detail)
}
