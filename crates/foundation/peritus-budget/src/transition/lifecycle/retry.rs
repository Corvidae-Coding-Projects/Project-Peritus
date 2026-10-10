//! Retry-history and attempt-charge validation.

use crate::{BudgetError, BudgetErrorKind, BudgetLedger, BudgetRequest};

pub(in crate::transition) fn retry_required(
    ledger: &BudgetLedger,
    request: BudgetRequest,
    end: usize,
) -> Result<bool, BudgetError> {
    let mut retry = false;
    let mut index = 0;
    while index < end {
        let prior = ledger.reservations[index];
        let same_revision = crate::identity_model::revision_equal(
            prior.request.verified_revision(),
            request.verified_revision(),
        );
        let same_action = crate::identity_model::action_id_equal(
            prior.request.verified_action_id(),
            request.verified_action_id(),
        );
        if same_revision && same_action {
            let same_digest = crate::identity_model::digest_equal(
                prior.request.verified_action_digest(),
                request.verified_action_digest(),
            );
            if !same_digest {
                let error = BudgetError::reservation(
                    BudgetErrorKind::BindingMismatch,
                    request.verified_reservation_id(),
                );
                return Err(error);
            }
            if prior.phase.is_live() {
                let error = BudgetError::reservation(
                    BudgetErrorKind::PriorAttemptUnresolved,
                    request.verified_reservation_id(),
                );
                return Err(error);
            }
            retry = true;
        }
        index += 1;
    }
    Ok(retry)
}
pub(in crate::transition) const fn validate_attempt_charge(
    request: BudgetRequest,
    retry: bool,
) -> Result<(), BudgetError> {
    let immediate_attempts =
        request.verified_consume_now().get(crate::BudgetDimension::Attempts).get();
    let immediate_retries =
        request.verified_consume_now().get(crate::BudgetDimension::Retries).get();
    let reserved_attempts = request.reserve().get(crate::BudgetDimension::Attempts).get();
    let reserved_retries = request.reserve().get(crate::BudgetDimension::Retries).get();
    if immediate_attempts != 1
        || immediate_retries != if retry { 1 } else { 0 }
        || reserved_attempts != 0
        || reserved_retries != 0
    {
        let error = BudgetError::reservation(
            BudgetErrorKind::InvalidAttemptAccounting,
            request.verified_reservation_id(),
        );
        return Err(error);
    }
    Ok(())
}
