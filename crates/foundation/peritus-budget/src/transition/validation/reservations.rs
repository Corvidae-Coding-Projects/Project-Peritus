//! Complete executable reservation-tombstone and retry-history validation.

use super::super::accounting::find_account;
use crate::{BudgetError, BudgetErrorKind, BudgetLedger};

pub(super) fn validate(ledger: &BudgetLedger) -> Result<(), BudgetError> {
    let mut index = 0;
    while index < ledger.reservations.len() {
        let record = ledger.reservations[index];
        let reservation_id = record.request.verified_reservation_id();
        super::structure::validate_reservation_unique_before(ledger, index)?;
        let account_index = find_account(ledger, record.request.budget_id()).ok_or_else(|| {
            BudgetError::reservation(BudgetErrorKind::CorruptState, reservation_id)
        })?;
        if !crate::identity_model::revision_equal(
            record.request.verified_revision(),
            ledger.accounts[account_index].revision,
        ) || !record.observed.fits_within(record.request.reserve())
        {
            return Err(BudgetError::reservation(BudgetErrorKind::CorruptState, reservation_id));
        }
        super::records::validate_record_phase(&record)?;
        let retry = match super::super::lifecycle::retry_required(ledger, record.request, index) {
            Ok(retry) => retry,
            Err(_error) => {
                return Err(BudgetError::reservation(
                    BudgetErrorKind::CorruptState,
                    reservation_id,
                ));
            }
        };
        if super::super::lifecycle::validate_attempt_charge(record.request, retry).is_err() {
            return Err(BudgetError::reservation(BudgetErrorKind::CorruptState, reservation_id));
        }
        index += 1;
    }
    Ok(())
}
