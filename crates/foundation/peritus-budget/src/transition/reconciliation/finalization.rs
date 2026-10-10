//! Exact and conservative reservation finalization transitions.

mod cancellation;

pub(in crate::transition) use self::cancellation::cancel_held;

use super::super::accounting::{
    bound_receipt, charge_lineage, establish_reserved_charge_safe, outstanding_validated,
    release_full_reservation, require_reference_binding, require_reservation,
};
use crate::{
    AmbiguousFinalization, BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger,
    BudgetOperation, BudgetReceipt, BudgetReceiptKind, ReservationPhase, ReservationReference,
};

pub(in crate::transition) fn settle_exact(
    ledger: &mut BudgetLedger,
    reference: ReservationReference,
) -> Result<BudgetReceipt, BudgetError> {
    let result = finalize_full_ceiling(
        ledger,
        reference,
        BudgetOperation::SettleExact,
        ReservationPhase::SettledExact,
        crate::BudgetCommand::SettleExact(reference),
    );
    result
}
pub(in crate::transition) fn finalize_ambiguous(
    ledger: &mut BudgetLedger,
    finalization: AmbiguousFinalization,
) -> Result<BudgetReceipt, BudgetError> {
    let concrete_reference = finalization.verified_reference();
    let result = finalize_full_ceiling(
        ledger,
        concrete_reference,
        BudgetOperation::FinalizeAmbiguous,
        ReservationPhase::SettledAmbiguous,
        crate::BudgetCommand::FinalizeAmbiguous(finalization),
    );
    result
}
fn finalize_full_ceiling(
    ledger: &mut BudgetLedger,
    reference: ReservationReference,
    operation: BudgetOperation,
    final_phase: ReservationPhase,
    command: crate::BudgetCommand,
) -> Result<BudgetReceipt, BudgetError> {
    let _ = command;
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
    if record.phase.equals(final_phase)
        && crate::identity_model::optional_digest_equal(
            record.final_evidence,
            Some(reference.verified_evidence_digest()),
        )
    {
        let budget_id = record.request.budget_id();
        let replay = bound_receipt(
            operation,
            BudgetReceiptKind::Idempotent,
            budget_id,
            reference.verified_reservation_id(),
            reference.verified_evidence_digest(),
        );
        return Ok(replay);
    }
    match record.phase {
        ReservationPhase::Active => {}
        _ => {
            let error = BudgetError::reservation(
                BudgetErrorKind::InvalidReservationPhase,
                reference.verified_reservation_id(),
            );
            return Err(error);
        }
    }
    let budget_id = record.request.budget_id();
    let remaining = outstanding_validated(record);
    BudgetAmounts::difference_le_left(remaining, record.request.reserve(), record.observed);
    establish_reserved_charge_safe(ledger, reservation_index, remaining);
    release_full_reservation(ledger, reservation_index, remaining);
    charge_lineage(ledger, budget_id, remaining)?;
    ledger.reservations[reservation_index].observed =
        ledger.reservations[reservation_index].request.reserve();
    ledger.reservations[reservation_index].phase = final_phase;
    ledger.reservations[reservation_index].final_evidence =
        Some(reference.verified_evidence_digest());
    let applied_receipt = BudgetReceipt::new(
        operation,
        BudgetReceiptKind::Applied,
        budget_id,
        Some(reference.verified_reservation_id()),
        remaining,
        BudgetAmounts::zero(),
        None,
        Some(reference.verified_evidence_digest()),
    );
    Ok(applied_receipt)
}
