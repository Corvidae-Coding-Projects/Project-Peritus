//! Executable exact folds for delegated and operation-reserved capacity.

mod reservations;
mod support;

pub(super) use self::reservations::direct_operation_reserved;
use self::support::{account_at, child_contributes};

use crate::{BudgetAmounts, BudgetError, BudgetLedger};

pub(super) fn direct_child_remaining(
    ledger: &BudgetLedger,
    account_index: usize,
) -> Result<BudgetAmounts, BudgetError> {
    if account_index >= ledger.accounts.len() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    let account = ledger.accounts[account_index];
    account.child_delegated_remaining.establish_bounds();
    let mut total = BudgetAmounts::zero();
    let mut index = 0;
    while index < ledger.accounts.len() {
        let child = account_at(ledger, index);
        let contributes = child_contributes(&child, account.id);
        if contributes {
            if !crate::identity_model::revision_equal(child.revision, account.revision) {
                return Err(crate::model::corrupt(child.id));
            }
            child.limits.amounts().establish_bounds();
            child.consumed.establish_bounds();
            child.operation_reserved.establish_bounds();
            child.child_delegated_remaining.establish_bounds();
            let remaining = match child.limits.amounts().checked_sub(child.consumed) {
                Ok(remaining) => remaining,
                Err(error) => {
                    return Err(crate::model::corrupt(child.id));
                }
            };
            crate::accounting_prefix_model::child_prefix_le_account(
                ledger,
                account_index,
                (index + 1) as usize,
            );
            ledger.accounts[account_index].child_delegated_remaining.establish_bounds();
            let next = match total.checked_add(remaining) {
                Ok(sum) => sum,
                Err(error) => {
                    return Err(crate::model::corrupt(account.id));
                }
            };
            total = next;
        } else {
        }
        index += 1;
    }
    Ok(total)
}
