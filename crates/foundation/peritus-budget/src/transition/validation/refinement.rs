//! Executable monotonicity, immutable-prefix, and ancestor-propagation checks.

mod propagation;

use self::propagation::validate_ancestor_propagation;

use crate::{BudgetError, BudgetErrorKind, BudgetLedger};

pub(super) fn validate(before: &BudgetLedger, after: &BudgetLedger) -> Result<(), BudgetError> {
    if !crate::identity_model::budget_id_equal(before.root_id, after.root_id) {
        return Err(crate::model::corrupt(after.root_id));
    }
    validate_accounts(before, after)?;
    validate_reservations(before, after)?;
    validate_ancestor_propagation(before, after)?;
    Ok(())
}
fn validate_accounts(before: &BudgetLedger, after: &BudgetLedger) -> Result<(), BudgetError> {
    if before.accounts.len() > after.accounts.len() {
        return Err(crate::model::corrupt(after.root_id));
    }
    let mut index = 0;
    while index < before.accounts.len() {
        let prior = account_at(before, index)?;
        let next = account_at(after, index)?;
        let same_id = crate::identity_model::budget_id_equal(prior.id, next.id);
        let same_parent = crate::identity_model::parent_equal(prior.parent_id, next.parent_id);
        let same_revision = crate::identity_model::revision_equal(prior.revision, next.revision);
        let same_limits = prior.limits.amounts().equals(next.limits.amounts());
        let monotonic = prior.consumed.fits_within(next.consumed);
        if !same_id || !same_parent || !same_revision || !same_limits || !monotonic {
            return Err(crate::model::corrupt(prior.id));
        }
        index += 1;
    }
    Ok(())
}
fn validate_reservations(before: &BudgetLedger, after: &BudgetLedger) -> Result<(), BudgetError> {
    if before.reservations.len() > after.reservations.len() {
        return Err(crate::model::corrupt(after.root_id));
    }
    let mut index = 0;
    while index < before.reservations.len() {
        let prior = reservation_at(before, index)?;
        let next = reservation_at(after, index)?;
        if !request_equal(prior.request, next.request) || !prior.observed.fits_within(next.observed)
        {
            return Err(BudgetError::reservation(
                BudgetErrorKind::CorruptState,
                prior.request.reservation_id(),
            ));
        }
        index += 1;
    }
    Ok(())
}
const fn request_equal(left: crate::BudgetRequest, right: crate::BudgetRequest) -> bool {
    crate::identity_model::reservation_id_equal(
        left.verified_reservation_id(),
        right.verified_reservation_id(),
    ) && crate::identity_model::budget_id_equal(left.budget_id(), right.budget_id())
        && crate::identity_model::revision_equal(
            left.verified_revision(),
            right.verified_revision(),
        )
        && crate::identity_model::action_id_equal(
            left.verified_action_id(),
            right.verified_action_id(),
        )
        && crate::identity_model::digest_equal(
            left.verified_action_digest(),
            right.verified_action_digest(),
        )
        && left.verified_consume_now().equals(right.verified_consume_now())
        && left.reserve().equals(right.reserve())
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
fn reservation_at(
    ledger: &BudgetLedger,
    index: usize,
) -> Result<crate::state::ReservationRecord, BudgetError> {
    if index >= ledger.reservations.len() {
        Err(crate::model::corrupt(ledger.root_id))
    } else {
        Ok(ledger.reservations[index])
    }
}
