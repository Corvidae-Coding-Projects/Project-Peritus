//! Atomic accounting and reservation insertion for a validated begin.

use super::super::accounting::{charge_lineage, establish_available_charge_safe};

use crate::{
    BudgetAmounts, BudgetError, BudgetLedger, BudgetOperation, BudgetReceipt, BudgetReceiptKind,
    BudgetRequest, ReservationPhase,
};

pub(super) fn apply_new_begin(
    ledger: &mut BudgetLedger,
    account_index: usize,
    request: BudgetRequest,
) -> Result<BudgetReceipt, BudgetError> {
    let consume_now = request.verified_consume_now();
    consume_now.establish_bounds();
    let starting_account = ledger.accounts[account_index];
    starting_account.limits.amounts().establish_bounds();
    starting_account.consumed.establish_bounds();
    starting_account.operation_reserved.establish_bounds();
    starting_account.child_delegated_remaining.establish_bounds();
    establish_available_charge_safe(ledger, account_index, request.budget_id(), consume_now);
    charge_lineage(ledger, request.budget_id(), consume_now)?;
    let reserve = request.reserve();
    ledger.accounts[account_index].operation_reserved.establish_bounds();
    reserve.establish_bounds();
    ledger.accounts[account_index].operation_reserved = ledger.accounts[account_index]
        .operation_reserved
        .checked_add(reserve)
        .map_err(BudgetError::arithmetic)?;
    let phase =
        if reserve.is_zero() { ReservationPhase::SettledExact } else { ReservationPhase::Held };
    ledger.reservations.push(crate::state::ReservationRecord {
        request,
        observed: BudgetAmounts::zero(),
        phase,
        activation_evidence: None,
        observation_evidence: None,
        final_evidence: None,
        final_reported: None,
        finality: None,
    });
    let applied_receipt = BudgetReceipt::new(
        BudgetOperation::Begin,
        BudgetReceiptKind::Applied,
        request.budget_id(),
        Some(request.verified_reservation_id()),
        consume_now,
        BudgetAmounts::zero(),
        None,
        None,
    );
    Ok(applied_receipt)
}
