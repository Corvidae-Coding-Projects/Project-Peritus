//! Account closure and release of unused delegated capacity.

use super::super::accounting::{find_account, has_live_work, receipt, require_account};
use crate::{
    BudgetAccountPhase, BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation,
    BudgetReceipt, BudgetReceiptKind,
};
use peritus_types::BudgetId;

pub(super) fn close(
    ledger: &mut BudgetLedger,
    budget_id: BudgetId,
) -> Result<BudgetReceipt, BudgetError> {
    super::super::validation::validate(ledger)?;
    close_validated(ledger, budget_id)
}
pub(in crate::transition) fn close_validated(
    ledger: &mut BudgetLedger,
    budget_id: BudgetId,
) -> Result<BudgetReceipt, BudgetError> {
    let account_index = match require_account(ledger, budget_id) {
        Ok(index) => index,
        Err(error) => {
            return Err(error);
        }
    };
    let initial_phase = ledger.accounts[account_index].phase;
    match initial_phase {
        BudgetAccountPhase::Draining | BudgetAccountPhase::Faulted => {}
        BudgetAccountPhase::Open => {
            let error = BudgetError::budget(BudgetErrorKind::InvalidAccountPhase, budget_id);
            return Err(error);
        }
        BudgetAccountPhase::Closed => {
            let replay_receipt =
                receipt(BudgetOperation::Close, BudgetReceiptKind::Idempotent, budget_id);
            return Ok(replay_receipt);
        }
    }
    if has_live_work(ledger, budget_id) {
        let error = BudgetError::budget(BudgetErrorKind::OutstandingWork, budget_id);
        return Err(error);
    }
    ledger.accounts[account_index].limits.amounts().establish_bounds();
    ledger.accounts[account_index].consumed.establish_bounds();
    ledger.accounts[account_index].operation_reserved.establish_bounds();
    ledger.accounts[account_index].child_delegated_remaining.establish_bounds();
    let unused = match ledger.accounts[account_index]
        .limits
        .amounts()
        .checked_sub(ledger.accounts[account_index].consumed)
    {
        Ok(unused) => unused,
        Err(error) => {
            return Err(BudgetError::arithmetic(error));
        }
    };
    let parent_id = ledger.accounts[account_index].parent_id;
    let parent_index = match parent_id {
        Some(parent_id) => {
            let index = match find_account(ledger, parent_id) {
                Some(index) => index,
                None => {
                    return Err(crate::model::corrupt(budget_id));
                }
            };
            Some(index)
        }
        None => None,
    };
    if let Some(parent_index) = parent_index {
        if parent_index == account_index {
            return Err(crate::model::corrupt(budget_id));
        }
        crate::accounting_model::child_unused_le_parent(
            ledger,
            account_index,
            parent_index,
            unused,
        );
        ledger.accounts[parent_index].child_delegated_remaining.establish_bounds();
        unused.establish_bounds();
        let delegated =
            match ledger.accounts[parent_index].child_delegated_remaining.checked_sub(unused) {
                Ok(delegated) => delegated,
                Err(arithmetic) => {
                    return Err(BudgetError::arithmetic(arithmetic));
                }
            };
        ledger.accounts[parent_index].child_delegated_remaining = delegated;
    }
    ledger.accounts[account_index].phase = BudgetAccountPhase::Closed;
    let close_receipt = BudgetReceipt::new(
        BudgetOperation::Close,
        BudgetReceiptKind::Applied,
        budget_id,
        None,
        BudgetAmounts::zero(),
        unused,
        None,
        None,
    );
    Ok(close_receipt)
}
