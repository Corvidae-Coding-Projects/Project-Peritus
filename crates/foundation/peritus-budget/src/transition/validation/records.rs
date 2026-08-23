//! Reservation tombstone phase validation.

use crate::{BudgetError, BudgetErrorKind, ReservationPhase};
use vstd::prelude::*;

verus! {

pub(super) const fn validate_record_phase(
    record: &crate::state::ReservationRecord,
) -> (result: Result<(), BudgetError>)
    ensures
        crate::invariant::record_phase_valid(*record) ==> result.is_ok(),
        match result {
            Ok(()) => crate::invariant::record_phase_valid(*record),
            Err(_) => true,
        },
{
    let valid = match record.phase {
        ReservationPhase::Held => {
            record.observed.is_zero()
                && record.activation_evidence.is_none()
                && record.observation_evidence.is_none()
                && record.final_evidence.is_none()
                && record.final_reported.is_none()
                && record.finality.is_none()
        }
        ReservationPhase::Active => {
            record.activation_evidence.is_some()
                && (record.observed.is_zero() || record.observation_evidence.is_some())
                && record.final_evidence.is_none()
                && record.final_reported.is_none()
                && record.finality.is_none()
        }
        ReservationPhase::SettledExact => {
            record.observed.equals(record.request.reserve())
                && record.final_reported.is_none()
                && record.finality.is_none()
                && (record.request.reserve().is_zero() || record.final_evidence.is_some())
        }
        ReservationPhase::SettledFinal => {
            record.activation_evidence.is_some()
                && crate::identity_model::optional_digest_equal(
                    record.observation_evidence,
                    record.final_evidence,
                )
                && record.final_evidence.is_some()
                && optional_amounts_equal(record.final_reported, Some(record.observed))
                && optional_finality_is_final(record.finality)
        }
        ReservationPhase::CancelledHeld => {
            record.observed.is_zero()
                && record.activation_evidence.is_none()
                && record.observation_evidence.is_none()
                && record.final_evidence.is_some()
                && record.final_reported.is_none()
                && record.finality.is_none()
        }
        ReservationPhase::SettledAmbiguous => {
            record.activation_evidence.is_some()
                && record.observed.equals(record.request.reserve())
                && record.final_evidence.is_some()
                && record.final_reported.is_none()
                && record.finality.is_none()
        }
        #[allow(clippy::option_if_let_else)]
        ReservationPhase::OverrunFaulted => {
            record.activation_evidence.is_some()
                && crate::identity_model::optional_digest_equal(
                    record.observation_evidence,
                    record.final_evidence,
                )
                && record.observed.equals(record.request.reserve())
                && record.final_evidence.is_some()
                && record.finality.is_some()
                && match record.final_reported {
                    Some(reported) => !reported.fits_within(record.request.reserve()),
                    None => false,
                }
        }
    };
    if !valid {
        return Err(BudgetError::reservation(
            BudgetErrorKind::CorruptState,
            record.request.reservation_id(),
        ));
    }
    assert(crate::invariant::record_phase_valid(*record));
    Ok(())
}

const fn optional_amounts_equal(
    left: Option<crate::BudgetAmounts>,
    right: Option<crate::BudgetAmounts>,
) -> (result: bool)
    ensures result == crate::invariant::optional_amounts_equal(left, right),
{
    match (left, right) {
        (Some(left_amount), Some(right_amount)) => left_amount.equals(right_amount),
        (None, None) => true,
        _ => false,
    }
}

const fn optional_finality_is_final(
    value: Option<crate::UsageFinality>,
) -> (result: bool)
    ensures result == crate::invariant::optional_finality_is_final(value),
{
    matches!(value, Some(crate::UsageFinality::Final))
}

} // verus!
