//! Executable uniqueness and parent-order checks with exact specification contracts.

use crate::{BudgetError, BudgetLedger};
use peritus_types::{BudgetId, BudgetReservationId};

fn duplicate_account_before(ledger: &BudgetLedger, budget_id: BudgetId, end: usize) -> bool {
    if end > ledger.accounts.len() {
        return true;
    }
    let mut index = 0;
    while index < end {
        if crate::identity_model::budget_id_equal(ledger.accounts[index].id, budget_id) {
            return true;
        }
        index += 1;
    }
    false
}
pub(super) fn validate_account_unique_before(
    ledger: &BudgetLedger,
    index: usize,
) -> Result<(), BudgetError> {
    if index >= ledger.accounts.len() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    let account = account_at(ledger, index)?;
    if duplicate_account_before(ledger, account.id, index) {
        return Err(crate::model::corrupt(account.id));
    }
    Ok(())
}
fn duplicate_reservation_before(
    ledger: &BudgetLedger,
    reservation_id: BudgetReservationId,
    end: usize,
) -> bool {
    if end > ledger.reservations.len() {
        return true;
    }
    let mut index = 0;
    while index < end {
        if crate::identity_model::reservation_id_equal(
            ledger.reservations[index].request.verified_reservation_id(),
            reservation_id,
        ) {
            return true;
        }
        index += 1;
    }
    false
}
pub(super) fn validate_reservation_unique_before(
    ledger: &BudgetLedger,
    index: usize,
) -> Result<(), BudgetError> {
    if index >= ledger.reservations.len() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    let request = ledger.reservations[index].request;
    let reservation_id = request.verified_reservation_id();
    if duplicate_reservation_before(ledger, reservation_id, index) {
        return Err(BudgetError::reservation(crate::BudgetErrorKind::CorruptState, reservation_id));
    }
    Ok(())
}
pub(super) fn validate_parent_before(
    ledger: &BudgetLedger,
    child_index: usize,
) -> Result<(), BudgetError> {
    if child_index == 0 || child_index >= ledger.accounts.len() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    let child = account_at(ledger, child_index)?;
    if child.parent_id.is_none() {
        return Err(crate::model::corrupt(child.id));
    }
    let mut parent_index = 0;
    while parent_index < child_index {
        if crate::identity_model::parent_matches_id(
            child.parent_id,
            ledger.accounts[parent_index].id,
        ) {
            if !crate::identity_model::revision_equal(
                child.revision,
                ledger.accounts[parent_index].revision,
            ) {
                return Err(crate::model::corrupt(child.id));
            }
            return Ok(());
        }
        parent_index += 1;
    }
    Err(crate::model::corrupt(child.id))
}
fn account_at(
    ledger: &BudgetLedger,
    index: usize,
) -> Result<crate::state::BudgetAccount, BudgetError> {
    if index >= ledger.accounts.len() {
        Err(crate::model::corrupt(ledger.root_id))
    } else {
        Ok(ledger.accounts[index])
    }
}
