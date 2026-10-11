//! Executable conservation helpers and their mathematical counterparts.

#![allow(
    clippy::missing_const_for_fn,
    clippy::too_many_lines,
    reason = "The executable conservation fold carries a line-by-line Verus postcondition proof"
)]

use crate::{BudgetAmounts, BudgetError, BudgetErrorKind};

use peritus_types::BudgetId;

#[allow(clippy::redundant_pub_crate)]
pub(crate) fn available(
    account: &crate::state::BudgetAccount,
) -> Result<BudgetAmounts, BudgetError> {
    account.limits.amounts().establish_bounds();
    account.consumed.establish_bounds();
    account.operation_reserved.establish_bounds();
    account.child_delegated_remaining.establish_bounds();
    let after_consumed = match account.limits.amounts().checked_sub(account.consumed) {
        Ok(after_consumed) => after_consumed,
        Err(arithmetic) => {
            return Err(BudgetError::arithmetic(arithmetic));
        }
    };
    let after_reserved = match after_consumed.checked_sub(account.operation_reserved) {
        Ok(after_reserved) => after_reserved,
        Err(arithmetic) => {
            return Err(BudgetError::arithmetic(arithmetic));
        }
    };
    let available = match after_reserved.checked_sub(account.child_delegated_remaining) {
        Ok(available) => available,
        Err(arithmetic) => {
            return Err(BudgetError::arithmetic(arithmetic));
        }
    };

    Ok(available)
}
#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module exact error contract"
)]
pub(crate) const fn corrupt(budget_id: BudgetId) -> BudgetError {
    BudgetError::budget(BudgetErrorKind::CorruptState, budget_id)
}
