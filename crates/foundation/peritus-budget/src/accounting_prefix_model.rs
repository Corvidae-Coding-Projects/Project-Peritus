//! Prefix bounds for executable reconstruction of derived accounting totals.

#![allow(
    unused_variables,
    clippy::redundant_pub_crate,
    clippy::semicolon_if_nothing_returned,
    clippy::unnecessary_cast,
    reason = "Verus prefix witnesses, proof blocks, and integer coercions erase from ordinary Rust"
)]

use crate::{BudgetDimension, BudgetLedger};

fn child_contribution_nonnegative(
    ledger: &BudgetLedger,
    parent: usize,
    dimension: BudgetDimension,
    index: usize,
) {
    ledger.accounts[index].limits.amounts().establish_bounds();
    ledger.accounts[index].consumed.establish_bounds();
    ledger.accounts[index].operation_reserved.establish_bounds();
    ledger.accounts[index].child_delegated_remaining.establish_bounds();
}
fn child_prefix_dimension_le_account(
    ledger: &BudgetLedger,
    parent: usize,
    dimension: BudgetDimension,
    end: usize,
) {
    if end < ledger.accounts.len() {
        child_prefix_dimension_le_account(ledger, parent, dimension, (end + 1) as usize);
        child_contribution_nonnegative(ledger, parent, dimension, end);
    }
}
pub(crate) fn child_prefix_le_account(ledger: &BudgetLedger, parent: usize, end: usize) {
    child_prefix_dimension_le_account(ledger, parent, BudgetDimension::ModelTokens, end);
    child_prefix_dimension_le_account(ledger, parent, BudgetDimension::ProviderCostMicrounits, end);
    child_prefix_dimension_le_account(
        ledger,
        parent,
        BudgetDimension::ActiveEffectMilliseconds,
        end,
    );
    child_prefix_dimension_le_account(ledger, parent, BudgetDimension::Attempts, end);
    child_prefix_dimension_le_account(ledger, parent, BudgetDimension::Retries, end);
}
fn reservation_contribution_nonnegative(
    ledger: &BudgetLedger,
    account: usize,
    dimension: BudgetDimension,
    index: usize,
) {
    ledger.reservations[index].request.reserve().establish_bounds();
    ledger.reservations[index].observed.establish_bounds();
}
fn reservation_prefix_dimension_le_account(
    ledger: &BudgetLedger,
    account: usize,
    dimension: BudgetDimension,
    end: usize,
) {
    if end < ledger.reservations.len() {
        reservation_prefix_dimension_le_account(ledger, account, dimension, (end + 1) as usize);
        reservation_contribution_nonnegative(ledger, account, dimension, end);
    }
}
pub(crate) fn reservation_prefix_le_account(ledger: &BudgetLedger, account: usize, end: usize) {
    reservation_prefix_dimension_le_account(ledger, account, BudgetDimension::ModelTokens, end);
    reservation_prefix_dimension_le_account(
        ledger,
        account,
        BudgetDimension::ProviderCostMicrounits,
        end,
    );
    reservation_prefix_dimension_le_account(
        ledger,
        account,
        BudgetDimension::ActiveEffectMilliseconds,
        end,
    );
    reservation_prefix_dimension_le_account(ledger, account, BudgetDimension::Attempts, end);
    reservation_prefix_dimension_le_account(ledger, account, BudgetDimension::Retries, end);
}
