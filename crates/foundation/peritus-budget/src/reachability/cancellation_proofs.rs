//! Exact held-reservation cancellation refinement.

#[cfg(verus_only)]
use crate::{BudgetCommand, BudgetLedger, BudgetReceipt};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn cancellation_receipt_exact(
    receipt: BudgetReceipt,
    reference: crate::ReservationReference,
    budget_id: peritus_types::BudgetId,
) -> bool {
    super::commands::receipt_identity(
        receipt,
        crate::BudgetOperation::CancelHeld,
        budget_id,
        Some(reference.spec_reservation_id()),
    )
        && receipt.spec_charged().spec_is_zero()
        && receipt.spec_reported().is_none()
        && crate::invariant::optional_digests_equal(
            receipt.spec_evidence_digest(),
            Some(reference.spec_evidence_digest()),
        )
}

pub(crate) open spec fn cancellation_record_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    reference: crate::ReservationReference,
) -> bool {
    super::reservations::cancellation_effect(
        before,
        after,
        reference.spec_reservation_id(),
        reference.spec_evidence_digest(),
    )
}

pub(crate) proof fn cancellation_refines(
    before: &BudgetLedger,
    after: &BudgetLedger,
    reference: crate::ReservationReference,
    receipt: BudgetReceipt,
    budget_id: peritus_types::BudgetId,
)
    requires
        super::guards::accepted_command_guard(
            before,
            BudgetCommand::CancelHeld(reference),
            receipt.spec_kind(),
        ),
        super::reservation_bound_to_budget(
            before,
            reference.spec_reservation_id(),
            budget_id,
        ),
        cancellation_receipt_exact(receipt, reference, budget_id),
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied
            || receipt.spec_kind() == crate::BudgetReceiptKind::Idempotent,
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied
            ==> (exists |index: int| #![auto]
                0 <= index < before.reservations@.len()
                    && crate::identity_model::reservation_ids_equal(
                        before.reservations[index].request.spec_reservation_id(),
                        reference.spec_reservation_id(),
                    )
                    && receipt.spec_released().spec_equal(
                        before.reservations[index].request.spec_reserve(),
                    ))
                && (exists |exact_released: crate::BudgetAmounts| #![auto]
                    exact_released.spec_equal(receipt.spec_released())
                        && super::account_updates::operation_release(
                        before,
                        after,
                        budget_id,
                        exact_released,
                    ))
                && cancellation_record_exact(before, after, reference),
        receipt.spec_kind() == crate::BudgetReceiptKind::Idempotent
            ==> receipt.spec_released().spec_is_zero()
                && super::ledgers_exactly_equal(before, after),
    ensures
        super::candidate_step(
            before,
            BudgetCommand::CancelHeld(reference),
            after,
            receipt,
        ),
{
    super::raw_step_is_accepted(
        before,
        BudgetCommand::CancelHeld(reference),
        after,
        receipt,
    );
}

} // verus!
