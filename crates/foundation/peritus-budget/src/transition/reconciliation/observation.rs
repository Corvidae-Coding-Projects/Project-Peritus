//! Monotonic high-water observation transitions.

use super::super::accounting::{require_binding, require_reservation};
use crate::{
    BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, ReservationPhase, UsageFinality, UsageObservation,
};

mod terminal;

mod in_bounds;

use in_bounds::{apply_final_validated, apply_interim_validated};
use terminal::{apply_overrun_validated, replay_final_observation_validated};

pub(in crate::transition) fn observe(
    ledger: &mut BudgetLedger,
    observation: UsageObservation,
) -> Result<BudgetReceipt, BudgetError> {
    super::super::validation::validate(ledger)?;
    observe_validated(ledger, observation)
}
pub(in crate::transition) fn observe_validated(
    ledger: &mut BudgetLedger,
    observation: UsageObservation,
) -> Result<BudgetReceipt, BudgetError> {
    let reservation_id = observation.verified_reservation_id();
    let reservation_index = match require_reservation(ledger, reservation_id) {
        Ok(index) => index,
        Err(error) => {
            return Err(error);
        }
    };
    let binding = require_binding(
        &ledger.reservations[reservation_index],
        observation.verified_action_id(),
        observation.verified_action_digest(),
    );
    if let Err(error) = binding {
        return Err(error);
    }
    match ledger.reservations[reservation_index].phase {
        ReservationPhase::Active => {}
        _ => {
            return replay_final_observation_validated(ledger, reservation_index, observation);
        }
    }
    let high_water = ledger.reservations[reservation_index].observed;
    let cumulative = observation.verified_cumulative();
    let evidence = observation.verified_evidence_digest();
    let finality = observation.verified_finality();
    let is_interim = match finality {
        UsageFinality::Interim => true,
        UsageFinality::Final => false,
    };
    if !high_water.fits_within(cumulative) {
        let error =
            BudgetError::reservation(BudgetErrorKind::NonmonotonicObservation, reservation_id);
        return Err(error);
    }
    if high_water.equals(cumulative)
        && ledger.reservations[reservation_index].observation_evidence.is_some()
        && !crate::identity_model::optional_digest_equal(
            ledger.reservations[reservation_index].observation_evidence,
            Some(evidence),
        )
    {
        let error = BudgetError::reservation(BudgetErrorKind::BindingMismatch, reservation_id);
        return Err(error);
    }
    if high_water.equals(cumulative)
        && crate::identity_model::optional_digest_equal(
            ledger.reservations[reservation_index].observation_evidence,
            Some(evidence),
        )
        && is_interim
    {
        let budget_id = ledger.reservations[reservation_index].request.budget_id();
        let replay_receipt = BudgetReceipt::new(
            BudgetOperation::ObserveUsage,
            BudgetReceiptKind::Idempotent,
            budget_id,
            Some(reservation_id),
            BudgetAmounts::zero(),
            BudgetAmounts::zero(),
            Some(cumulative),
            Some(evidence),
        );
        return Ok(replay_receipt);
    }
    let ceiling = ledger.reservations[reservation_index].request.reserve();
    if !cumulative.fits_within(ceiling) {
        return apply_overrun_validated(ledger, reservation_index, observation);
    }
    match finality {
        UsageFinality::Final => apply_final_validated(ledger, reservation_index, observation),
        UsageFinality::Interim => apply_interim_validated(ledger, reservation_index, observation),
    }
}
