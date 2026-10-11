//! Recursive executable witness for chargeable account-lineage credit.

use crate::{BudgetAmounts, BudgetLedger};
use peritus_types::BudgetId;

pub(super) fn establish_account_credit_safe(
    ledger: &BudgetLedger,
    index: usize,
    current_id: BudgetId,
    amount: BudgetAmounts,
    delegated_child: bool,
    available_credit: bool,
    fuel: usize,
) {
    let account = ledger.accounts[index];
    account.limits.amounts().establish_bounds();
    account.consumed.establish_bounds();
    account.operation_reserved.establish_bounds();
    account.child_delegated_remaining.establish_bounds();
    amount.establish_bounds();
    match account.parent_id {
        None => {}
        Some(parent_id) => {
            let parent_index = match super::super::find_account(ledger, parent_id) {
                Some(parent) => parent,
                None => {
                    return;
                }
            };
            let unused = match account.limits.amounts().checked_sub(account.consumed) {
                Ok(unused) => unused,
                Err(arithmetic) => {
                    return;
                }
            };
            crate::accounting_model::child_unused_le_parent(ledger, index, parent_index, unused);

            establish_account_credit_safe(
                ledger,
                parent_index,
                parent_id,
                amount,
                true,
                false,
                fuel - 1,
            );
        }
    }
}
