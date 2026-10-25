//! Complete executable account-tree and conservation validation.

use super::super::accounting::{find_account, has_live_work};
use crate::{BudgetError, BudgetLedger};

pub(super) fn validate(ledger: &BudgetLedger) -> Result<(), BudgetError> {
    if ledger.accounts.is_empty() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    let root_index = find_account(ledger, ledger.root_id)
        .ok_or_else(|| crate::model::corrupt(ledger.root_id))?;
    if root_index != 0 || ledger.accounts[root_index].parent_id.is_some() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    let mut index = 0;
    while index < ledger.accounts.len() {
        let account = ledger.accounts[index];
        super::structure::validate_account_unique_before(ledger, index)?;
        if index > 0 {
            super::structure::validate_parent_before(ledger, index)?;
        }
        crate::model::available(&account).map_err(|_error| crate::model::corrupt(account.id))?;
        let direct_child_remaining = super::sums::direct_child_remaining(ledger, index)?;
        if !direct_child_remaining.equals(account.child_delegated_remaining) {
            return Err(crate::model::corrupt(account.id));
        }
        let direct_operation_reserved = super::sums::direct_operation_reserved(ledger, index)?;
        if !direct_operation_reserved.equals(account.operation_reserved) {
            return Err(crate::model::corrupt(account.id));
        }
        if account.phase.is_closed() && has_live_work(ledger, account.id) {
            return Err(crate::model::corrupt(account.id));
        }
        index += 1;
    }
    Ok(())
}
