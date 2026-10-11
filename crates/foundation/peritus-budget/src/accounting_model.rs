//! Exact specification folds for derived child and reservation accounting.

#![allow(
    unused_variables,
    clippy::only_used_in_recursion,
    clippy::redundant_pub_crate,
    clippy::semicolon_if_nothing_returned,
    reason = "Recursive accounting witnesses are consumed by Verus specifications and erase from ordinary Rust"
)]

mod aggregation;

pub(crate) use self::aggregation::child_unused_le_parent;

use crate::{BudgetDimension, BudgetLedger};
use peritus_types::BudgetId;

pub(crate) fn child_sum_nonnegative(
    ledger: &BudgetLedger,
    parent_id: BudgetId,
    dimension: BudgetDimension,
    end: usize,
) {
    if end > 0 {
        child_sum_nonnegative(ledger, parent_id, dimension, end - 1);
        ledger.accounts[end - 1].limits.amounts().establish_bounds();
        ledger.accounts[end - 1].consumed.establish_bounds();
        ledger.accounts[end - 1].operation_reserved.establish_bounds();
        ledger.accounts[end - 1].child_delegated_remaining.establish_bounds();
    }
}
pub(crate) fn child_contribution_le_sum(
    ledger: &BudgetLedger,
    parent_id: BudgetId,
    dimension: BudgetDimension,
    index: usize,
    end: usize,
) {
    let last = end - 1;
    ledger.accounts[last].limits.amounts().establish_bounds();
    ledger.accounts[last].consumed.establish_bounds();
    ledger.accounts[last].operation_reserved.establish_bounds();
    ledger.accounts[last].child_delegated_remaining.establish_bounds();
    if index == last {
        child_sum_nonnegative(ledger, parent_id, dimension, last);
    } else {
        child_contribution_le_sum(ledger, parent_id, dimension, index, last);
    }
}
