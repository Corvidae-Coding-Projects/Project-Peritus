//! Recursive exact consumption propagation through one account lineage.

use super::find_account;

use crate::{BudgetAmounts, BudgetError, BudgetLedger};
use peritus_types::BudgetId;

pub(in crate::transition) fn charge_lineage(
    ledger: &mut BudgetLedger,
    budget_id: BudgetId,
    amount: BudgetAmounts,
) -> Result<(), BudgetError> {
    let account_count = ledger.accounts.len();
    let result = charge_lineage_inner(ledger, budget_id, amount, false, account_count);
    result
}
fn charge_lineage_inner(
    ledger: &mut BudgetLedger,
    current_id: BudgetId,
    amount: BudgetAmounts,
    delegated_child: bool,
    remaining_steps: usize,
) -> Result<(), BudgetError> {
    if remaining_steps == 0 {
        return Err(crate::model::corrupt(current_id));
    }
    let index = match find_account(ledger, current_id) {
        Some(index) => index,
        None => {
            return Err(crate::model::corrupt(current_id));
        }
    };
    let parent_id = ledger.accounts[index].parent_id;
    if delegated_child {
        ledger.accounts[index].child_delegated_remaining.establish_bounds();
        amount.establish_bounds();
        let delegated = match ledger.accounts[index].child_delegated_remaining.checked_sub(amount) {
            Ok(delegated) => delegated,
            Err(arithmetic) => {
                return Err(BudgetError::arithmetic(arithmetic));
            }
        };
        ledger.accounts[index].child_delegated_remaining = delegated;
    }
    let consumed = match ledger.accounts[index].consumed.checked_add(amount) {
        Ok(consumed) => consumed,
        Err(arithmetic) => {
            return Err(BudgetError::arithmetic(arithmetic));
        }
    };
    ledger.accounts[index].consumed = consumed;
    if let Some(parent_id) = parent_id {
        charge_lineage_inner(ledger, parent_id, amount, true, remaining_steps - 1)?;
    }
    Ok(())
}
