//! Executable tree, tombstone, conservation, and refinement validation.

use crate::{BudgetError, BudgetLedger};

mod accounts;
mod arithmetic;
mod records;
mod refinement;
mod reservations;
mod structure;
mod sums;

pub(super) fn validate(ledger: &BudgetLedger) -> Result<(), BudgetError> {
    accounts::validate(ledger)?;
    reservations::validate(ledger)?;
    Ok(())
}
pub(super) fn validate_refinement(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> Result<(), BudgetError> {
    refinement::validate(before, after)
}
