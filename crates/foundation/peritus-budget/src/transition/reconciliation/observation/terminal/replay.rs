//! Exact replay of terminal observations.

use crate::{
    BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, ReservationPhase, UsageObservation,
};

pub(in crate::transition) fn replay_final_observation_validated(
    ledger: &BudgetLedger,
    reservation_index: usize,
    observation: UsageObservation,
) -> Result<BudgetReceipt, BudgetError> {
    let record = &ledger.reservations[reservation_index];
    let reported_matches = match record.final_reported {
        Some(reported) => reported.equals(observation.verified_cumulative()),
        None => false,
    };
    let finality_matches = match (record.finality, observation.verified_finality()) {
        (Some(crate::UsageFinality::Interim), crate::UsageFinality::Interim)
        | (Some(crate::UsageFinality::Final), crate::UsageFinality::Final) => true,
        _ => false,
    };
    let exact_replay = finality_matches
        && crate::identity_model::optional_digest_equal(
            record.final_evidence,
            Some(observation.verified_evidence_digest()),
        )
        && reported_matches;
    if !exact_replay {
        let error = BudgetError::reservation(
            BudgetErrorKind::InvalidReservationPhase,
            observation.verified_reservation_id(),
        );
        return Err(error);
    }
    let replay_receipt = match record.phase {
        ReservationPhase::SettledFinal => {
            let receipt = BudgetReceipt::new(
                BudgetOperation::ObserveUsage,
                BudgetReceiptKind::Idempotent,
                record.request.budget_id(),
                Some(observation.verified_reservation_id()),
                BudgetAmounts::zero(),
                BudgetAmounts::zero(),
                Some(observation.verified_cumulative()),
                Some(observation.verified_evidence_digest()),
            );
            receipt
        }
        ReservationPhase::OverrunFaulted => {
            let receipt = BudgetReceipt::new(
                BudgetOperation::ObserveUsage,
                BudgetReceiptKind::OverrunFaulted,
                record.request.budget_id(),
                Some(observation.verified_reservation_id()),
                BudgetAmounts::zero(),
                BudgetAmounts::zero(),
                Some(observation.verified_cumulative()),
                Some(observation.verified_evidence_digest()),
            );
            receipt
        }
        _ => {
            let error = BudgetError::reservation(
                BudgetErrorKind::InvalidReservationPhase,
                observation.verified_reservation_id(),
            );
            return Err(error);
        }
    };
    Ok(replay_receipt)
}
