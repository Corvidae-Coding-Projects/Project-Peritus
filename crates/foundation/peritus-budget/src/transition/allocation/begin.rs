//! Validation and exact branch selection for operation begin.

use super::super::accounting::{find_reservation, require_open_lineage};
use super::super::lifecycle::{retry_required, validate_attempt_charge};
use super::begin_apply::apply_new_begin;
use crate::{
    BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, BudgetRequest,
};

pub(in crate::transition) fn begin(
    ledger: &mut BudgetLedger,
    request: BudgetRequest,
) -> Result<BudgetReceipt, BudgetError> {
    let existing_reservation = find_reservation(ledger, request.verified_reservation_id());
    match existing_reservation {
        Some(existing_index) => {
            let existing = &ledger.reservations[existing_index];
            if existing.request.matches(request) {
                let replay = BudgetReceipt::new(
                    BudgetOperation::Begin,
                    BudgetReceiptKind::Idempotent,
                    request.budget_id(),
                    Some(request.verified_reservation_id()),
                    BudgetAmounts::zero(),
                    BudgetAmounts::zero(),
                    None,
                    None,
                );
                return Ok(replay);
            }
            let error = BudgetError::reservation(
                BudgetErrorKind::DuplicateReservationConflict,
                request.verified_reservation_id(),
            );
            return Err(error);
        }
        None => {}
    }
    let consume_now = request.verified_consume_now();
    consume_now.establish_bounds();
    let reserve = request.reserve();
    let requested = match consume_now.checked_add(reserve) {
        Ok(requested) => requested,
        Err(arithmetic) => {
            let error = BudgetError::arithmetic(arithmetic);
            return Err(error);
        }
    };
    if requested.is_zero() {
        let error = BudgetError::reservation(
            BudgetErrorKind::EmptyRequest,
            request.verified_reservation_id(),
        );
        BudgetAmounts::zero_sum_has_zero_operands(
            requested,
            request.verified_consume_now(),
            request.reserve(),
        );
        return Err(error);
    }
    let account_index = match require_open_lineage(ledger, request.budget_id()) {
        Ok(account_index) => account_index,
        Err(error) => {
            return Err(error);
        }
    };
    if !crate::identity_model::revision_equal(
        ledger.accounts[account_index].revision,
        request.verified_revision(),
    ) {
        let error = BudgetError::reservation(
            BudgetErrorKind::BindingMismatch,
            request.verified_reservation_id(),
        );
        return Err(error);
    }
    let retry = match retry_required(ledger, request, ledger.reservations.len()) {
        Ok(retry) => retry,
        Err(error) => {
            return Err(error);
        }
    };
    match validate_attempt_charge(request, retry) {
        Ok(()) => {}
        Err(error) => {
            return Err(error);
        }
    }
    let available = match crate::model::available(&ledger.accounts[account_index]) {
        Ok(available) => available,
        Err(error) => {
            return Err(error);
        }
    };
    if !requested.fits_within(available) {
        let dimensions = requested.exceeding_dimensions(available);
        let error = BudgetError::insufficient(request.budget_id(), dimensions);
        return Err(error);
    }
    request.reserve().establish_bounds();
    apply_new_begin(ledger, account_index, request)
}
