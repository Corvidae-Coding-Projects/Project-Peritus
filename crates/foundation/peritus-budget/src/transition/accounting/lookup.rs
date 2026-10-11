//! Exact account/reservation lookup and live-work validation.

use crate::{BudgetError, BudgetErrorKind, BudgetLedger, ReservationReference};
use peritus_types::{ActionId, BudgetId, BudgetReservationId, Sha256Digest};

pub(in crate::transition) fn has_live_work(ledger: &BudgetLedger, budget_id: BudgetId) -> bool {
    let mut reservation_index = 0;
    while reservation_index < ledger.reservations.len() {
        let record = &ledger.reservations[reservation_index];
        let matches_budget =
            crate::identity_model::budget_id_equal(record.request.budget_id(), budget_id);
        let live = record.phase.is_live();
        if matches_budget && live {
            return true;
        }
        reservation_index += 1;
    }
    let mut account_index = 0;
    while account_index < ledger.accounts.len() {
        let account = &ledger.accounts[account_index];
        let matches_parent = crate::identity_model::parent_matches_id(account.parent_id, budget_id);
        let closed = account.phase.is_closed();
        if matches_parent && !closed {
            return true;
        }
        account_index += 1;
    }
    false
}
pub(in crate::transition) fn require_binding(
    record: &crate::state::ReservationRecord,
    action_id: ActionId,
    action_digest: Sha256Digest,
) -> Result<(), BudgetError> {
    if !crate::identity_model::action_id_equal(record.request.verified_action_id(), action_id)
        || !crate::identity_model::digest_equal(
            record.request.verified_action_digest(),
            action_digest,
        )
    {
        return Err(BudgetError::reservation(
            BudgetErrorKind::BindingMismatch,
            record.request.verified_reservation_id(),
        ));
    }
    Ok(())
}
pub(in crate::transition) fn require_reference_binding(
    record: &crate::state::ReservationRecord,
    reference: ReservationReference,
) -> Result<(), BudgetError> {
    require_binding(record, reference.verified_action_id(), reference.verified_action_digest())
}
pub(in crate::transition) fn require_account(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
) -> Result<usize, BudgetError> {
    match find_account(ledger, budget_id) {
        Some(index) => Ok(index),
        None => Err(BudgetError::budget(BudgetErrorKind::UnknownBudget, budget_id)),
    }
}
pub(in crate::transition) fn require_reservation(
    ledger: &BudgetLedger,
    reservation_id: BudgetReservationId,
) -> Result<usize, BudgetError> {
    match find_reservation(ledger, reservation_id) {
        Some(index) => Ok(index),
        None => Err(BudgetError::reservation(BudgetErrorKind::UnknownReservation, reservation_id)),
    }
}
pub(in crate::transition) fn find_account(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
) -> Option<usize> {
    let mut index = 0;
    while index < ledger.accounts.len() {
        if crate::identity_model::budget_id_equal(ledger.accounts[index].id, budget_id) {
            return Some(index);
        }
        index += 1;
    }
    None
}
pub(in crate::transition) fn find_reservation(
    ledger: &BudgetLedger,
    reservation_id: BudgetReservationId,
) -> Option<usize> {
    let mut index = 0;
    while index < ledger.reservations.len() {
        if crate::identity_model::reservation_id_equal(
            ledger.reservations[index].request.verified_reservation_id(),
            reservation_id,
        ) {
            return Some(index);
        }
        index += 1;
    }
    None
}
