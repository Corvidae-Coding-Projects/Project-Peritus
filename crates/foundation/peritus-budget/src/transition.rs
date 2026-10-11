//! Total value-in/value-out command reducer.

#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    clippy::large_types_passed_by_value,
    clippy::manual_let_else,
    clippy::match_like_matches_macro,
    clippy::missing_const_for_fn,
    clippy::only_used_in_recursion,
    clippy::option_if_let_else,
    clippy::semicolon_if_nothing_returned,
    clippy::single_match,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::unnecessary_cast,
    reason = "Verus reducer contracts and ghost blocks require syntax ordinary Clippy measures after proof erasure"
)]

use crate::{BudgetCommand, BudgetError, BudgetLedger, BudgetSnapshot, ReservationSnapshot};
use peritus_types::{BudgetId, BudgetReservationId};

mod accounting;
mod allocation;
mod lifecycle;
mod reconciliation;
mod reducer;
mod validation;

#[allow(clippy::large_types_passed_by_value)]
#[allow(clippy::redundant_pub_crate)]
pub(crate) fn apply(
    ledger: &BudgetLedger,
    command: BudgetCommand,
) -> Result<crate::BudgetTransition, BudgetError> {
    let input_validation = validation::validate(ledger);
    if input_validation.is_err() {
        let error = crate::model::corrupt(ledger.root_id);
        return Err(error);
    }
    let mut next = ledger.duplicate();
    let reducer_result = reducer::apply_validated(&mut next, command);
    let receipt = match reducer_result {
        Ok(receipt) => receipt,
        Err(error) => {
            return Err(error);
        }
    };
    let output_validation = validation::validate(&next);
    if output_validation.is_err() {
        let error = crate::model::corrupt(ledger.root_id);
        return Err(error);
    }
    let refinement_validation = validation::validate_refinement(ledger, &next);
    if refinement_validation.is_err() {
        let error = crate::model::corrupt(ledger.root_id);
        return Err(error);
    }
    let transition = crate::BudgetTransition::new(next, receipt);
    Ok(transition)
}
pub fn snapshot_account(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
) -> Result<BudgetSnapshot, BudgetError> {
    match validation::validate(ledger) {
        Ok(()) => {}
        Err(_) => {
            let error = crate::model::corrupt(ledger.root_id);
            return Err(error);
        }
    }
    let index = match accounting::find_account(ledger, budget_id) {
        Some(index) => index,
        None => {
            let error = BudgetError::budget(crate::BudgetErrorKind::UnknownBudget, budget_id);
            return Err(error);
        }
    };
    let account = &ledger.accounts[index];
    let available = match crate::model::available(account) {
        Ok(available) => available,
        Err(_) => {
            return Err(crate::model::corrupt(ledger.root_id));
        }
    };
    let snapshot = BudgetSnapshot::from_account(account, available);
    Ok(snapshot)
}
pub fn snapshot_reservation(
    ledger: &BudgetLedger,
    reservation_id: BudgetReservationId,
) -> Result<ReservationSnapshot, BudgetError> {
    match validation::validate(ledger) {
        Ok(()) => {}
        Err(_) => {
            let error = crate::model::corrupt(ledger.root_id);
            return Err(error);
        }
    }
    let index = match accounting::find_reservation(ledger, reservation_id) {
        Some(index) => index,
        None => {
            let error = BudgetError::reservation(
                crate::BudgetErrorKind::UnknownReservation,
                reservation_id,
            );
            return Err(error);
        }
    };
    let record = &ledger.reservations[index];
    let outstanding = match accounting::outstanding(record) {
        Ok(outstanding) => outstanding,
        Err(_) => {
            return Err(crate::model::corrupt(ledger.root_id));
        }
    };
    let snapshot = ReservationSnapshot::from_record(record, outstanding);
    Ok(snapshot)
}
pub fn validate(ledger: &BudgetLedger) -> Result<(), BudgetError> {
    match validation::validate(ledger) {
        Ok(()) => Ok(()),
        Err(_) => {
            let error = crate::model::corrupt(ledger.root_id);
            Err(error)
        }
    }
}
