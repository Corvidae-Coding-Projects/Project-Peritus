//! Plan validation and terminal materialization settlement transitions.

use crate::{
    aggregate::{
        AggregateError, DeliveryState, HarnessCommand, HarnessState, ReconciliationDecision,
    },
    materialization::{
        MaterializationFailure, MaterializationPlan, MaterializationPlanId, MaterializationReason,
        MaterializationReceipt,
    },
};

use super::{advance, conflict, domain, invalid, limit};

pub(super) fn validate_plan(
    state: &HarnessState,
    plan: &MaterializationPlan,
) -> Result<(), AggregateError> {
    let revision = state
        .history
        .revision(plan.revision_digest())
        .ok_or_else(|| invalid("plan revision is absent from history"))?;
    if revision.harness_id() != plan.harness_id()
        || revision.number().get() != plan.revision_number()
        || revision.graph().graph_digest().digest() != plan.graph_digest()
    {
        return Err(conflict("plan revision or graph binding differs from history"));
    }
    if state.pending.contains_key(&plan.id()) {
        return Err(conflict("plan identity is already pending"));
    }
    if state
        .pending
        .values()
        .any(|pending| pending.plan().target().workspace_id() == plan.target().workspace_id())
    {
        return Err(invalid("another plan is pending for the target workspace"));
    }
    if let MaterializationReason::Rollback { source_revision, .. } = plan.reason() {
        state
            .history
            .validate_rollback(*source_revision, plan.revision_digest())
            .map_err(domain)?;
    }
    Ok(())
}

pub(super) fn settle_receipt(
    prior: &HarnessState,
    receipt: &MaterializationReceipt,
    command: &HarnessCommand,
    sequence: u64,
) -> Result<HarnessState, AggregateError> {
    let mut state = prior.clone();
    let pending = state
        .pending
        .remove(&receipt.plan_id())
        .ok_or_else(|| invalid("receipt names no pending plan"))?;
    if pending.plan().digest() != receipt.plan_digest()
        || pending.plan().revision_digest() != receipt.revision_digest()
        || pending.plan().target() != receipt.before()
    {
        return Err(conflict("receipt differs from the exact pending plan"));
    }
    if state.receipts.contains_key(&receipt.id()) {
        return Err(conflict("receipt identity is already retained"));
    }
    if u64::try_from(state.receipts.len()).unwrap_or(u64::MAX) >= state.limits.max_receipt_history()
    {
        return Err(limit("hot receipt history is full; retire a settled receipt first"));
    }
    state.receipts.insert(receipt.id(), receipt.clone());
    advance(&mut state, command, sequence);
    Ok(state)
}

pub(super) fn reconcile(
    prior: &HarnessState,
    plan_id: MaterializationPlanId,
    decision: &ReconciliationDecision,
    command: &HarnessCommand,
    sequence: u64,
) -> Result<HarnessState, AggregateError> {
    if !prior.pending.contains_key(&plan_id) {
        return Err(invalid("reconciliation names no pending plan"));
    }
    match decision {
        ReconciliationDecision::Retry => {
            let mut state = prior.clone();
            state
                .pending
                .get_mut(&plan_id)
                .ok_or_else(|| invalid("reconciliation plan disappeared during reduction"))?
                .delivery = DeliveryState::Pending;
            advance(&mut state, command, sequence);
            Ok(state)
        }
        ReconciliationDecision::Completed(receipt) if receipt.plan_id() == plan_id => {
            settle_receipt(prior, receipt, command, sequence)
        }
        ReconciliationDecision::Conflict(failure) if failure.plan_id() == plan_id => {
            let mut state = prior.clone();
            state.pending.remove(&plan_id);
            retain_failure(&mut state, failure.clone())?;
            advance(&mut state, command, sequence);
            Ok(state)
        }
        _ => Err(conflict("reconciliation evidence names another plan")),
    }
}

pub(super) fn retain_failure(
    state: &mut HarnessState,
    failure: MaterializationFailure,
) -> Result<(), AggregateError> {
    if u64::try_from(state.failures.len()).unwrap_or(u64::MAX)
        >= state.limits.max_retained_diagnostics()
    {
        return Err(limit("retained materialization diagnostics are full"));
    }
    state.failures.push(failure);
    Ok(())
}
