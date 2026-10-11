//! Interim in-ceiling usage application.

use crate::transition::accounting::{
    charge_lineage, establish_observation_charge_safe, release_observation_charge, require_binding,
    require_reservation,
};
use crate::{
    BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, ReservationPhase, UsageFinality, UsageObservation,
};

pub(in crate::transition) fn apply_interim(
    ledger: &mut BudgetLedger,
    observation: UsageObservation,
) -> Result<BudgetReceipt, BudgetError> {
    super::super::super::super::validation::validate(ledger)?;
    let index = require_reservation(ledger, observation.verified_reservation_id())?;
    require_binding(
        &ledger.reservations[index],
        observation.verified_action_id(),
        observation.verified_action_digest(),
    )?;
    let phase = ledger.reservations[index].phase;
    match phase {
        ReservationPhase::Active => {}
        _ => {
            return Err(BudgetError::reservation(
                BudgetErrorKind::InvalidReservationPhase,
                observation.verified_reservation_id(),
            ));
        }
    }
    let finality = observation.verified_finality();
    match finality {
        UsageFinality::Interim => {}
        UsageFinality::Final => {
            return Err(BudgetError::reservation(
                BudgetErrorKind::InvalidReservationPhase,
                observation.verified_reservation_id(),
            ));
        }
    }
    let record = &ledger.reservations[index];
    if !record.observed.fits_within(observation.verified_cumulative()) {
        return Err(BudgetError::reservation(
            BudgetErrorKind::NonmonotonicObservation,
            observation.verified_reservation_id(),
        ));
    }
    if !observation.verified_cumulative().fits_within(record.request.reserve()) {
        return Err(crate::model::corrupt(record.request.budget_id()));
    }
    let evidence = observation.verified_evidence_digest();
    if record.observed.equals(observation.verified_cumulative())
        && record.observation_evidence.is_some()
        && !crate::identity_model::optional_digest_equal(
            record.observation_evidence,
            Some(evidence),
        )
    {
        return Err(BudgetError::reservation(
            BudgetErrorKind::BindingMismatch,
            observation.verified_reservation_id(),
        ));
    }
    if record.observed.equals(observation.verified_cumulative())
        && crate::identity_model::optional_digest_equal(record.observation_evidence, Some(evidence))
    {
        let receipt = BudgetReceipt::new(
            BudgetOperation::ObserveUsage,
            BudgetReceiptKind::Idempotent,
            record.request.budget_id(),
            Some(observation.verified_reservation_id()),
            BudgetAmounts::zero(),
            BudgetAmounts::zero(),
            Some(observation.verified_cumulative()),
            Some(evidence),
        );
        return Ok(receipt);
    }
    apply_interim_validated(ledger, index, observation)
}
pub(in crate::transition) fn apply_interim_validated(
    ledger: &mut BudgetLedger,
    reservation_index: usize,
    observation: UsageObservation,
) -> Result<BudgetReceipt, BudgetError> {
    let phase = ledger.reservations[reservation_index].phase;
    match phase {
        ReservationPhase::Active => {}
        _ => {
            return Err(BudgetError::reservation(
                BudgetErrorKind::InvalidReservationPhase,
                observation.verified_reservation_id(),
            ));
        }
    }
    let finality = observation.verified_finality();
    match finality {
        UsageFinality::Interim => {}
        UsageFinality::Final => {
            return Err(BudgetError::reservation(
                BudgetErrorKind::InvalidReservationPhase,
                observation.verified_reservation_id(),
            ));
        }
    }
    let record = &ledger.reservations[reservation_index];
    let high_water = record.observed;
    let cumulative = observation.verified_cumulative();
    let evidence = observation.verified_evidence_digest();
    let budget_id = record.request.budget_id();
    let delta = match cumulative.checked_sub(high_water) {
        Ok(delta) => delta,
        Err(error) => return Err(BudgetError::arithmetic(error)),
    };
    establish_observation_charge_safe(ledger, reservation_index, cumulative, delta);
    release_observation_charge(ledger, reservation_index, delta);
    charge_lineage(ledger, budget_id, delta)?;
    ledger.reservations[reservation_index].observed = cumulative;
    ledger.reservations[reservation_index].observation_evidence = Some(evidence);
    let receipt = BudgetReceipt::new(
        BudgetOperation::ObserveUsage,
        BudgetReceiptKind::Applied,
        budget_id,
        Some(observation.verified_reservation_id()),
        delta,
        BudgetAmounts::zero(),
        Some(cumulative),
        Some(evidence),
    );
    Ok(receipt)
}
