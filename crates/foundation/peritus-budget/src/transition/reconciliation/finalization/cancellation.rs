//! Held-reservation cancellation and exact capacity release.

use crate::transition::accounting::{
    bound_receipt, release_full_reservation, require_reference_binding, require_reservation,
};
use crate::{
    BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, ReservationPhase, ReservationReference,
};

pub(in crate::transition) fn cancel_held(
    ledger: &mut BudgetLedger,
    reference: ReservationReference,
) -> Result<BudgetReceipt, BudgetError> {
    let reservation_index = match require_reservation(ledger, reference.verified_reservation_id()) {
        Ok(index) => index,
        Err(error) => {
            return Err(error);
        }
    };
    match require_reference_binding(&ledger.reservations[reservation_index], reference) {
        Ok(()) => {}
        Err(error) => {
            return Err(error);
        }
    }
    let record = &ledger.reservations[reservation_index];
    if record.phase.equals(ReservationPhase::CancelledHeld)
        && crate::identity_model::optional_digest_equal(
            record.final_evidence,
            Some(reference.verified_evidence_digest()),
        )
    {
        let replay_receipt = bound_receipt(
            BudgetOperation::CancelHeld,
            BudgetReceiptKind::Idempotent,
            record.request.budget_id(),
            reference.verified_reservation_id(),
            reference.verified_evidence_digest(),
        );
        return Ok(replay_receipt);
    }
    match record.phase {
        ReservationPhase::Held => {}
        _ => {
            let error = BudgetError::reservation(
                BudgetErrorKind::InvalidReservationPhase,
                reference.verified_reservation_id(),
            );
            return Err(error);
        }
    }
    let budget_id = record.request.budget_id();
    let released = record.request.reserve();
    release_full_reservation(ledger, reservation_index, released);
    ledger.reservations[reservation_index].phase = ReservationPhase::CancelledHeld;
    ledger.reservations[reservation_index].final_evidence =
        Some(reference.verified_evidence_digest());
    let applied_receipt = BudgetReceipt::new(
        BudgetOperation::CancelHeld,
        BudgetReceiptKind::Applied,
        budget_id,
        Some(reference.verified_reservation_id()),
        BudgetAmounts::zero(),
        released,
        None,
        Some(reference.verified_evidence_digest()),
    );
    Ok(applied_receipt)
}
