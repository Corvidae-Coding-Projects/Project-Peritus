//! Uniqueness consequences of the ledger's prefix invariants.

use super::{
    account_entry_valid, account_unique_before, ledger_account_structure_holds,
    ledger_reservation_structure_holds, reservation_entry_valid, reservation_unique_before,
};
use crate::BudgetLedger;
use vstd::prelude::*;

verus! {

pub(crate) proof fn matching_accounts_are_unique(
    ledger: &BudgetLedger,
    left: int,
    right: int,
)
    requires
        ledger_account_structure_holds(ledger),
        0 <= left < ledger.accounts@.len(),
        0 <= right < ledger.accounts@.len(),
        crate::identity_model::budget_ids_equal(
            ledger.accounts[left].id,
            ledger.accounts[right].id,
        ),
    ensures left == right,
{
    if left < right {
        assert(account_entry_valid(ledger, right));
        assert(account_unique_before(ledger, right));
    } else if right < left {
        assert(account_entry_valid(ledger, left));
        assert(account_unique_before(ledger, left));
    }
}

pub(crate) proof fn matching_reservations_are_unique(
    ledger: &BudgetLedger,
    left: int,
    right: int,
)
    requires
        ledger_reservation_structure_holds(ledger),
        0 <= left < ledger.reservations@.len(),
        0 <= right < ledger.reservations@.len(),
        crate::identity_model::reservation_ids_equal(
            ledger.reservations[left].request.spec_reservation_id(),
            ledger.reservations[right].request.spec_reservation_id(),
        ),
    ensures left == right,
{
    if left < right {
        assert(reservation_entry_valid(ledger, right));
        assert(reservation_unique_before(ledger, right));
    } else if right < left {
        assert(reservation_entry_valid(ledger, left));
        assert(reservation_unique_before(ledger, left));
    }
}

} // verus!
