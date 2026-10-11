//! In-ceiling interim and final usage application.

mod interim;

pub(in crate::transition) use self::interim::{apply_interim, apply_interim_validated};

use super::super::super::accounting::{
    charge_lineage, establish_observation_charge_safe, find_account, outstanding_validated,
    release_observation_charge, release_operation_reservation_validated, require_binding,
    require_reservation,
};
use crate::{
    BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, ReservationPhase, UsageFinality, UsageObservation,
};

pub(in crate::transition) fn apply_final(
    ledger: &mut BudgetLedger,
    observation: UsageObservation,
) -> Result<BudgetReceipt, BudgetError> {
    super::super::super::validation::validate(ledger)?;
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
        UsageFinality::Final => {}
        UsageFinality::Interim => {
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
    if record.observed.equals(observation.verified_cumulative())
        && record.observation_evidence.is_some()
        && !crate::identity_model::optional_digest_equal(
            record.observation_evidence,
            Some(observation.verified_evidence_digest()),
        )
    {
        return Err(BudgetError::reservation(
            BudgetErrorKind::BindingMismatch,
            observation.verified_reservation_id(),
        ));
    }
    apply_final_validated(ledger, index, observation)
}
pub(in crate::transition) fn apply_final_validated(
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
        UsageFinality::Final => {}
        UsageFinality::Interim => {
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
    let ceiling = record.request.reserve();
    let account_index = match find_account(ledger, budget_id) {
        Some(index) => index,
        None => {
            return Err(crate::model::corrupt(budget_id));
        }
    };
    let full_outstanding = outstanding_validated(record);
    let delta = match cumulative.checked_sub(high_water) {
        Ok(delta) => delta,
        Err(error) => return Err(BudgetError::arithmetic(error)),
    };
    establish_observation_charge_safe(ledger, reservation_index, cumulative, delta);
    release_observation_charge(ledger, reservation_index, delta);
    charge_lineage(ledger, budget_id, delta)?;
    ledger.reservations[reservation_index].observed = cumulative;
    ledger.reservations[reservation_index].observation_evidence = Some(evidence);
    let released = match ceiling.checked_sub(cumulative) {
        Ok(released) => released,
        Err(error) => return Err(BudgetError::arithmetic(error)),
    };
    release_operation_reservation_validated(ledger, account_index, budget_id, released);
    ledger.reservations[reservation_index].phase = ReservationPhase::SettledFinal;
    ledger.reservations[reservation_index].final_evidence = Some(evidence);
    ledger.reservations[reservation_index].final_reported = Some(cumulative);
    ledger.reservations[reservation_index].finality = Some(UsageFinality::Final);
    let receipt = BudgetReceipt::new(
        BudgetOperation::ObserveUsage,
        BudgetReceiptKind::Applied,
        budget_id,
        Some(observation.verified_reservation_id()),
        delta,
        released,
        Some(cumulative),
        Some(evidence),
    );
    Ok(receipt)
}
