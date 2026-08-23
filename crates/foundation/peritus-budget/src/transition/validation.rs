//! Executable tree, tombstone, conservation, and refinement validation.

use crate::{BudgetError, BudgetLedger};
use vstd::prelude::*;

mod accounts;
mod arithmetic;
mod records;
mod refinement;
mod reservations;
mod structure;
mod sums;

verus! {

pub(super) fn validate(ledger: &BudgetLedger) -> (result: Result<(), BudgetError>)
    ensures
        crate::model::ledger_well_formed(ledger) ==> result.is_ok(),
        match result {
            Ok(()) => crate::model::ledger_well_formed(ledger),
            Err(_) => true,
        },
{
    accounts::validate(ledger)?;
    reservations::validate(ledger)?;
    assert(crate::invariant::ledger_structure_holds(ledger));
    assert(crate::model::ledger_well_formed(ledger));
    Ok(())
}

pub(super) fn validate_refinement(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> (result: Result<(), BudgetError>)
    ensures
        (crate::model::ledger_well_formed(before)
            && crate::reachability::complete_refinement(before, after)) ==> result.is_ok(),
        match result {
            Ok(()) => {
                crate::model::ledger_consumption_monotonic(before, after)
                    && crate::model::ledger_high_water_monotonic(before, after)
                    && crate::refinement_model::ledger_identity_stable(before, after)
                    && crate::refinement_model::ancestor_consumption_propagates(before, after)
            }
            Err(_) => true,
        },
{
    refinement::validate(before, after)
}

} // verus!
