//! Executable bridges from reserved or available capacity to lineage charge safety.

use super::credit::establish_account_credit_safe;

use crate::{BudgetAmounts, BudgetLedger};
use peritus_types::BudgetId;

pub(in crate::transition) fn establish_reserved_charge_safe(
    ledger: &BudgetLedger,
    reservation_index: usize,
    amount: BudgetAmounts,
) {
    let budget_id = ledger.reservations[reservation_index].request.budget_id();
    let account_index = match super::super::find_account(ledger, budget_id) {
        Some(index) => index,
        None => {
            return;
        }
    };
    let full = super::super::outstanding_validated(&ledger.reservations[reservation_index]);

    establish_account_credit_safe(
        ledger,
        account_index,
        budget_id,
        amount,
        false,
        false,
        ledger.accounts.len(),
    );
}
pub(in crate::transition) fn establish_available_charge_safe(
    ledger: &BudgetLedger,
    account_index: usize,
    budget_id: BudgetId,
    amount: BudgetAmounts,
) {
    establish_account_credit_safe(
        ledger,
        account_index,
        budget_id,
        amount,
        false,
        true,
        ledger.accounts.len(),
    );
}
pub(in crate::transition) fn establish_observation_charge_safe(
    ledger: &BudgetLedger,
    reservation_index: usize,
    cumulative: BudgetAmounts,
    delta: BudgetAmounts,
) {
    let reserve = ledger.reservations[reservation_index].request.reserve();
    let observed = ledger.reservations[reservation_index].observed;
    reserve.establish_bounds();
    observed.establish_bounds();
    cumulative.establish_bounds();
    delta.establish_bounds();
    establish_reserved_charge_safe(ledger, reservation_index, delta);
}
