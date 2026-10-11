//! Open-lineage validation and exact reservation-outstanding queries.

use super::require_account;
use crate::{BudgetAccountPhase, BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger};
use peritus_types::BudgetId;

pub(in crate::transition) fn require_open_lineage(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
) -> Result<usize, BudgetError> {
    let target = match require_account(ledger, budget_id) {
        Ok(index) => index,
        Err(error) => {
            return Err(error);
        }
    };
    match require_open_parent_chain(ledger, target) {
        Ok(()) => {}
        Err(error) => {
            return Err(error);
        }
    }
    Ok(target)
}
fn require_open_parent_chain(ledger: &BudgetLedger, index: usize) -> Result<(), BudgetError> {
    let account = ledger.accounts[index];
    match account.phase {
        BudgetAccountPhase::Open => {}
        BudgetAccountPhase::Draining | BudgetAccountPhase::Faulted | BudgetAccountPhase::Closed => {
            let error = BudgetError::budget(BudgetErrorKind::AccountNotOpen, account.id);
            return Err(error);
        }
    }
    match account.parent_id {
        None => {}
        Some(parent_id) => {
            let parent_index = match require_account(ledger, parent_id) {
                Ok(parent_index) => parent_index,
                Err(error) => {
                    return Err(error);
                }
            };
            let parent_result = require_open_parent_chain(ledger, parent_index);
            match parent_result {
                Ok(()) => {}
                Err(error) => {
                    return Err(error);
                }
            }
        }
    }
    Ok(())
}
pub(in crate::transition) const fn outstanding(
    record: &crate::state::ReservationRecord,
) -> Result<BudgetAmounts, BudgetError> {
    let live = record.phase.is_live();
    let amount = if live {
        record.request.reserve().establish_bounds();
        record.observed.establish_bounds();
        match record.request.reserve().checked_sub(record.observed) {
            Ok(amount) => amount,
            Err(error) => {
                return Err(BudgetError::arithmetic(error));
            }
        }
    } else {
        let zero = BudgetAmounts::zero();
        zero
    };
    Ok(amount)
}
pub(in crate::transition) const fn outstanding_validated(
    record: &crate::state::ReservationRecord,
) -> BudgetAmounts {
    match outstanding(record) {
        Ok(amount) => amount,
        Err(_) => BudgetAmounts::zero(),
    }
}
