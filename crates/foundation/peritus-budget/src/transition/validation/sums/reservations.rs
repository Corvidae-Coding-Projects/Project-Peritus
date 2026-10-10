//! Exact fold for live operation-reserved capacity.

use super::super::super::accounting::outstanding;
use super::support::{reservation_at, reservation_contributes};

use crate::{BudgetAmounts, BudgetError, BudgetLedger};

pub(in crate::transition::validation) fn direct_operation_reserved(
    ledger: &BudgetLedger,
    account_index: usize,
) -> Result<BudgetAmounts, BudgetError> {
    if account_index >= ledger.accounts.len() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    let budget_id = ledger.accounts[account_index].id;
    ledger.accounts[account_index].operation_reserved.establish_bounds();
    let mut total = BudgetAmounts::zero();
    let mut index = 0;
    while index < ledger.reservations.len() {
        let reservation = reservation_at(ledger, index);
        let contributes = reservation_contributes(&reservation, budget_id);
        if contributes {
            let amount = match outstanding(&reservation) {
                Ok(amount) => amount,
                Err(_error) => {
                    return Err(crate::model::corrupt(budget_id));
                }
            };
            crate::accounting_prefix_model::reservation_prefix_le_account(
                ledger,
                account_index,
                (index + 1) as usize,
            );
            ledger.accounts[account_index].operation_reserved.establish_bounds();
            let next = match total.checked_add(amount) {
                Ok(sum) => sum,
                Err(error) => {
                    return Err(crate::model::corrupt(budget_id));
                }
            };
            total = next;
        } else {
        }
        index += 1;
    }
    Ok(total)
}
