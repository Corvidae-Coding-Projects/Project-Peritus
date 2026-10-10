//! Exact release of outstanding operation reservations.

use super::{find_account, outstanding_validated};

use crate::{BudgetAmounts, BudgetError, BudgetLedger};
use peritus_types::BudgetId;

#[allow(
    dead_code,
    reason = "retained exact checked primitive for transition-local reservation release"
)]
pub(in crate::transition) fn release_operation_reservation(
    ledger: &mut BudgetLedger,
    budget_id: BudgetId,
    amount: BudgetAmounts,
) -> Result<(), BudgetError> {
    let index = match find_account(ledger, budget_id) {
        Some(index) => index,
        None => return Err(crate::model::corrupt(budget_id)),
    };
    ledger.accounts[index].operation_reserved = ledger.accounts[index]
        .operation_reserved
        .checked_sub(amount)
        .map_err(BudgetError::arithmetic)?;
    Ok(())
}
pub(in crate::transition) fn release_operation_reservation_validated(
    ledger: &mut BudgetLedger,
    account_index: usize,
    _budget_id: BudgetId,
    amount: BudgetAmounts,
) {
    ledger.accounts[account_index].operation_reserved.establish_bounds();
    amount.establish_bounds();
    let after = match ledger.accounts[account_index].operation_reserved.checked_sub(amount) {
        Ok(after) => after,
        Err(_arithmetic) => {
            return;
        }
    };
    ledger.accounts[account_index].operation_reserved = after;
}
pub(in crate::transition) fn release_observation_charge(
    ledger: &mut BudgetLedger,
    reservation_index: usize,
    amount: BudgetAmounts,
) {
    let budget_id = ledger.reservations[reservation_index].request.budget_id();
    let account_index = match find_account(ledger, budget_id) {
        Some(index) => index,
        None => {
            return;
        }
    };
    let _full = outstanding_validated(&ledger.reservations[reservation_index]);
    ledger.accounts[account_index].operation_reserved.establish_bounds();
    amount.establish_bounds();
    let after = match ledger.accounts[account_index].operation_reserved.checked_sub(amount) {
        Ok(after) => after,
        Err(_arithmetic) => {
            return;
        }
    };
    ledger.accounts[account_index].operation_reserved = after;
}
pub(in crate::transition) fn release_full_reservation(
    ledger: &mut BudgetLedger,
    reservation_index: usize,
    amount: BudgetAmounts,
) {
    let budget_id = ledger.reservations[reservation_index].request.budget_id();
    let account_index = match find_account(ledger, budget_id) {
        Some(index) => index,
        None => {
            return;
        }
    };
    ledger.accounts[account_index].operation_reserved.establish_bounds();
    amount.establish_bounds();
    let after = match ledger.accounts[account_index].operation_reserved.checked_sub(amount) {
        Ok(after) => after,
        Err(_arithmetic) => {
            return;
        }
    };
    ledger.accounts[account_index].operation_reserved = after;
}
