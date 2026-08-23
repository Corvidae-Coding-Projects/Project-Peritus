//! Held-reservation cancellation and exact capacity release.

use crate::transition::accounting::{
    bound_receipt, release_full_reservation, require_reference_binding, require_reservation,
};
use crate::{
    BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, ReservationPhase, ReservationReference,
};
use vstd::prelude::*;

verus! {

pub(in crate::transition) fn cancel_held(
    ledger: &mut BudgetLedger,
    reference: ReservationReference,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires crate::model::ledger_well_formed(old(ledger)),
    ensures
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                crate::BudgetCommand::CancelHeld(reference),
                final(ledger),
                receipt,
            ),
            Err(error) => crate::reachability::rejection_cause(
                old(ledger),
                crate::BudgetCommand::CancelHeld(reference),
                error,
            ),
        },
{
    let ghost before = *ledger;
    let reservation_index = match require_reservation(
        ledger,
        reference.verified_reservation_id(),
    ) {
        Ok(index) => index,
        Err(error) => {
            assert(crate::reachability::rejection_cause(
                ledger,
                crate::BudgetCommand::CancelHeld(reference),
                error,
            ));
            return Err(error);
        }
    };
    match require_reference_binding(&ledger.reservations[reservation_index], reference) {
        Ok(()) => {}
        Err(error) => {
            assert(crate::reachability::reservation_at_guard(
                ledger,
                reference.spec_reservation_id(),
                reservation_index as int,
            ));
            assert(!crate::reachability::reference_binding_guard(
                ledger.reservations[reservation_index as int],
                reference,
            ));
            assert(crate::reachability::rejection_cause(
                ledger,
                crate::BudgetCommand::CancelHeld(reference),
                error,
            ));
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
        proof {
            crate::reachability::ledger_exact_reflexive(ledger);
            assert(crate::reachability::reservation_bound_to_budget(
                &before,
                reference.spec_reservation_id(),
                record.request.spec_budget_id(),
            ));
            assert(crate::reachability::cancellation_receipt_exact(
                replay_receipt,
                reference,
                record.request.spec_budget_id(),
            ));
            crate::reachability::cancellation_guard_from_runtime(
                &before,
                reference,
                BudgetReceiptKind::Idempotent,
                reservation_index as int,
            );
            assert(replay_receipt.spec_kind() == BudgetReceiptKind::Idempotent);
            crate::reachability::cancellation_refines(
                &before,
                ledger,
                reference,
                replay_receipt,
                record.request.spec_budget_id(),
            );
        }
        return Ok(replay_receipt);
    }
    match record.phase {
        ReservationPhase::Held => {}
        _ => {
            let error = BudgetError::reservation(
                BudgetErrorKind::InvalidReservationPhase,
                reference.verified_reservation_id(),
            );
            assert(crate::reachability::reservation_at_guard(
                &before,
                reference.spec_reservation_id(),
                reservation_index as int,
            ));
            assert(crate::reachability::reference_binding_guard(
                before.reservations[reservation_index as int],
                reference,
            ));
            assert(crate::reachability::rejection_cause(
                &before,
                crate::BudgetCommand::CancelHeld(reference),
                error,
            ));
            return Err(error);
        }
    }
    let budget_id = record.request.budget_id();
    let released = record.request.reserve();
    assert(crate::invariant::ledger_structure_holds(&before));
    assert(crate::invariant::reservation_entry_valid(
        &before,
        reservation_index as int,
    ));
    assert(crate::invariant::record_phase_valid(
        before.reservations[reservation_index as int],
    ));
    assert(BudgetAmounts::spec_difference(
        released,
        before.reservations[reservation_index as int].request.spec_reserve(),
        before.reservations[reservation_index as int].observed,
    ));
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
    proof {
        assert(crate::reachability::reservation_bound_to_budget(
            &before,
            reference.spec_reservation_id(),
            budget_id,
        ));
        assert(crate::reachability::cancellation_receipt_exact(
            applied_receipt,
            reference,
            budget_id,
        ));
        assert(released.spec_equal(applied_receipt.spec_released()));
        assert(exists |exact_released: BudgetAmounts| #![auto]
            exact_released.spec_equal(applied_receipt.spec_released())
                && crate::reachability::operation_release_exact(
                    &before,
                    ledger,
                    budget_id,
                    exact_released,
                ));
        assert(ledger.reservations@ == before.reservations@.update(
            reservation_index as int,
            ledger.reservations[reservation_index as int],
        ));
        assert(crate::reachability::cancellation_record_exact(
            &before,
            ledger,
            reference,
        ));
        crate::reachability::cancellation_guard_from_runtime(
            &before,
            reference,
            BudgetReceiptKind::Applied,
            reservation_index as int,
        );
        crate::reachability::cancellation_refines(
            &before,
            ledger,
            reference,
            applied_receipt,
            budget_id,
        );
    }
    Ok(applied_receipt)
}

} // verus!
