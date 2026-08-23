//! Exact finalization and account-lifecycle command steps.

#[cfg(verus_only)]
use crate::{
    BudgetAmounts, BudgetCommand, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, ReservationPhase,
};
#[cfg(verus_only)]
use peritus_types::{BudgetId, BudgetReservationId, Sha256Digest};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn finalization_receipt(
    receipt: BudgetReceipt,
    operation: BudgetOperation,
    budget_id: BudgetId,
    reservation_id: BudgetReservationId,
    evidence: Sha256Digest,
) -> bool {
    super::commands::receipt_identity(
        receipt,
        operation,
        budget_id,
        Some(reservation_id),
    )
        && receipt.spec_released().spec_is_zero()
        && receipt.spec_reported().is_none()
        && crate::invariant::optional_digests_equal(
            receipt.spec_evidence_digest(),
            Some(evidence),
        )
}

pub(crate) open spec fn full_finalization_step(
    before: &BudgetLedger,
    reference: crate::ReservationReference,
    operation: BudgetOperation,
    phase: ReservationPhase,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
) -> bool {
    exists |budget_id: BudgetId| #![auto]
        super::commands::bound_budget(before, reference.spec_reservation_id(), budget_id)
            && finalization_receipt(
                receipt,
                operation,
                budget_id,
                reference.spec_reservation_id(),
                reference.spec_evidence_digest(),
            )
            && match receipt.spec_kind() {
                BudgetReceiptKind::Idempotent => {
                    receipt.spec_charged().spec_is_zero()
                        && super::commands::ledgers_exactly_equal(before, after)
                }
                BudgetReceiptKind::Applied => {
                    (exists |index: int| #![auto]
                        0 <= index < before.reservations@.len()
                            && crate::identity_model::reservation_ids_equal(
                                before.reservations[index].request.spec_reservation_id(),
                                reference.spec_reservation_id(),
                            )
                            && BudgetAmounts::spec_difference(
                                receipt.spec_charged(),
                                before.reservations[index].request.spec_reserve(),
                                before.reservations[index].observed,
                            ))
                        && (exists |released_state: BudgetLedger, exact_charged: BudgetAmounts| #![auto]
                            exact_charged.spec_equal(receipt.spec_charged())
                                && super::account_updates::full_charge_accounting(
                                before,
                                after,
                                &released_state,
                                budget_id,
                                exact_charged,
                            ))
                        && super::reservations::full_finalization_effect(
                            before,
                            after,
                            reference.spec_reservation_id(),
                            reference.spec_evidence_digest(),
                            phase,
                        )
                }
                BudgetReceiptKind::OverrunFaulted => false,
            }
}

pub(crate) open spec fn cancel_held_step(
    before: &BudgetLedger,
    reference: crate::ReservationReference,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
) -> bool {
    exists |budget_id: BudgetId| #![auto]
        super::commands::bound_budget(before, reference.spec_reservation_id(), budget_id)
            && super::commands::receipt_identity(
                receipt,
                BudgetOperation::CancelHeld,
                budget_id,
                Some(reference.spec_reservation_id()),
            )
            && receipt.spec_charged().spec_is_zero()
            && receipt.spec_reported().is_none()
            && crate::invariant::optional_digests_equal(
                receipt.spec_evidence_digest(),
                Some(reference.spec_evidence_digest()),
            )
            && match receipt.spec_kind() {
                BudgetReceiptKind::Idempotent => {
                    receipt.spec_released().spec_is_zero()
                        && super::commands::ledgers_exactly_equal(before, after)
                }
                BudgetReceiptKind::Applied => {
                    (exists |index: int| #![auto]
                        0 <= index < before.reservations@.len()
                            && crate::identity_model::reservation_ids_equal(
                                before.reservations[index].request.spec_reservation_id(),
                                reference.spec_reservation_id(),
                            )
                            && receipt.spec_released().spec_equal(
                                before.reservations[index].request.spec_reserve(),
                            ))
                        && (exists |exact_released: BudgetAmounts| #![auto]
                            exact_released.spec_equal(receipt.spec_released())
                                && super::account_updates::operation_release(
                                before,
                                after,
                                budget_id,
                                exact_released,
                            ))
                        && super::reservations::cancellation_effect(
                            before,
                            after,
                            reference.spec_reservation_id(),
                            reference.spec_evidence_digest(),
                        )
                }
                BudgetReceiptKind::OverrunFaulted => false,
            }
}

pub(crate) open spec fn seal_step(
    before: &BudgetLedger,
    budget_id: BudgetId,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
) -> bool {
    super::commands::receipt_identity(
        receipt,
        BudgetOperation::Seal,
        budget_id,
        None,
    )
        && receipt.spec_charged().spec_is_zero()
        && receipt.spec_released().spec_is_zero()
        && super::commands::receipt_has_no_observation(receipt)
        && super::commands::reservations_exactly_equal(before, after)
        && match receipt.spec_kind() {
            BudgetReceiptKind::Idempotent => {
                super::commands::ledgers_exactly_equal(before, after)
            }
            BudgetReceiptKind::Applied => super::accounts::account_phase_effect(
                before,
                after,
                budget_id,
                crate::BudgetAccountPhase::Draining,
            ),
            BudgetReceiptKind::OverrunFaulted => false,
        }
}

pub(crate) open spec fn close_step(
    before: &BudgetLedger,
    budget_id: BudgetId,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
) -> bool {
    super::commands::receipt_identity(
        receipt,
        BudgetOperation::Close,
        budget_id,
        None,
    )
        && receipt.spec_charged().spec_is_zero()
        && super::commands::receipt_has_no_observation(receipt)
        && match receipt.spec_kind() {
            BudgetReceiptKind::Idempotent => {
                receipt.spec_released().spec_is_zero()
                    && super::commands::ledgers_exactly_equal(before, after)
            }
            BudgetReceiptKind::Applied => super::allocation::close_account_effect(
                before,
                after,
                budget_id,
                receipt.spec_released(),
            ),
            BudgetReceiptKind::OverrunFaulted => false,
        }
}

pub(crate) open spec fn lifecycle_step(
    before: &BudgetLedger,
    command: BudgetCommand,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
) -> bool {
    match command {
        BudgetCommand::SettleExact(reference) => full_finalization_step(
            before,
            reference,
            BudgetOperation::SettleExact,
            ReservationPhase::SettledExact,
            after,
            receipt,
        ),
        BudgetCommand::CancelHeld(reference) => {
            cancel_held_step(before, reference, after, receipt)
        }
        BudgetCommand::FinalizeAmbiguous(finalization) => full_finalization_step(
            before,
            finalization.spec_reference(),
            BudgetOperation::FinalizeAmbiguous,
            ReservationPhase::SettledAmbiguous,
            after,
            receipt,
        ),
        BudgetCommand::Seal(budget_id) => seal_step(before, budget_id, after, receipt),
        BudgetCommand::Close(budget_id) => close_step(before, budget_id, after, receipt),
        _ => false,
    }
}

} // verus!
