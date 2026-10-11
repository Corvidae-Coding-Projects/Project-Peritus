//! Shared executable projections and proof bindings for accounting folds.

use crate::{BudgetAccountPhase, BudgetAmounts, BudgetLedger};
use peritus_types::BudgetId;

pub(super) fn account_at(ledger: &BudgetLedger, index: usize) -> crate::state::BudgetAccount {
    ledger.accounts[index]
}
pub(super) fn reservation_at(
    ledger: &BudgetLedger,
    index: usize,
) -> crate::state::ReservationRecord {
    ledger.reservations[index]
}
pub(super) const fn child_contributes(
    child: &crate::state::BudgetAccount,
    parent_id: BudgetId,
) -> bool {
    let parent_matches = if let Some(actual) = child.parent_id {
        crate::identity_model::budget_id_equal(actual, parent_id)
    } else {
        false
    };
    let not_closed = match child.phase {
        BudgetAccountPhase::Closed => false,
        BudgetAccountPhase::Open | BudgetAccountPhase::Draining | BudgetAccountPhase::Faulted => {
            true
        }
    };
    parent_matches && not_closed
}
pub(super) const fn reservation_contributes(
    reservation: &crate::state::ReservationRecord,
    budget_id: BudgetId,
) -> bool {
    crate::identity_model::budget_id_equal(reservation.request.budget_id(), budget_id)
}
