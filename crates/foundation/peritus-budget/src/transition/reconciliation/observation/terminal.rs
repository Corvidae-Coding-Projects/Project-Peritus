//! Overrun and exact terminal-observation reconciliation.

mod replay;

pub(in crate::transition) use self::replay::replay_final_observation_validated;

use super::super::super::accounting::{
    charge_lineage, establish_fault_lineage_safe, establish_reserved_charge_safe, fault_lineage,
    find_account, outstanding_validated, release_full_reservation, require_binding,
    require_reservation,
};

use crate::{
    BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, ReservationPhase, UsageObservation,
};

pub(in crate::transition) fn apply_overrun(
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
    let record = &ledger.reservations[index];
    if !record.observed.fits_within(observation.verified_cumulative()) {
        return Err(BudgetError::reservation(
            BudgetErrorKind::NonmonotonicObservation,
            observation.verified_reservation_id(),
        ));
    }
    if observation.verified_cumulative().fits_within(record.request.reserve()) {
        return Err(crate::model::corrupt(record.request.budget_id()));
    }
    apply_overrun_validated(ledger, index, observation)
}
pub(in crate::transition) fn apply_overrun_validated(
    ledger: &mut BudgetLedger,
    reservation_index: usize,
    observation: UsageObservation,
) -> Result<BudgetReceipt, BudgetError> {
    match ledger.reservations[reservation_index].phase {
        ReservationPhase::Active => {}
        _ => {
            return Err(BudgetError::reservation(
                BudgetErrorKind::InvalidReservationPhase,
                observation.verified_reservation_id(),
            ));
        }
    }
    let record = &ledger.reservations[reservation_index];
    let budget_id = record.request.budget_id();
    let ceiling = record.request.reserve();
    let account_index = match find_account(ledger, budget_id) {
        Some(index) => index,
        None => {
            return Err(crate::model::corrupt(budget_id));
        }
    };
    establish_fault_lineage_safe(ledger, account_index, budget_id, ledger.accounts.len());
    let remaining = outstanding_validated(record);
    BudgetAmounts::difference_le_left(remaining, record.request.reserve(), record.observed);
    establish_reserved_charge_safe(ledger, reservation_index, remaining);
    release_full_reservation(ledger, reservation_index, remaining);
    charge_lineage(ledger, budget_id, remaining)?;
    ledger.reservations[reservation_index].observed = ceiling;
    ledger.reservations[reservation_index].observation_evidence =
        Some(observation.verified_evidence_digest());
    ledger.reservations[reservation_index].phase = ReservationPhase::OverrunFaulted;
    ledger.reservations[reservation_index].final_evidence =
        Some(observation.verified_evidence_digest());
    ledger.reservations[reservation_index].final_reported = Some(observation.verified_cumulative());
    ledger.reservations[reservation_index].finality = Some(observation.verified_finality());
    fault_lineage(ledger, budget_id)?;
    let overrun_receipt = make_overrun_receipt(observation, budget_id, remaining);
    Ok(overrun_receipt)
}
const fn make_overrun_receipt(
    observation: UsageObservation,
    budget_id: peritus_types::BudgetId,
    charged: BudgetAmounts,
) -> BudgetReceipt {
    BudgetReceipt::new(
        BudgetOperation::ObserveUsage,
        BudgetReceiptKind::OverrunFaulted,
        budget_id,
        Some(observation.verified_reservation_id()),
        charged,
        BudgetAmounts::zero(),
        Some(observation.verified_cumulative()),
        Some(observation.verified_evidence_digest()),
    )
}
