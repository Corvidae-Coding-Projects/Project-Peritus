//! Exact and conservative reservation finalization transitions.

mod cancellation;

pub(in crate::transition) use self::cancellation::cancel_held;

use super::super::accounting::{
    bound_receipt, charge_lineage, establish_reserved_charge_safe, outstanding_validated,
    release_full_reservation,
    require_reference_binding, require_reservation,
};
use crate::{
    AmbiguousFinalization, BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger,
    BudgetOperation, BudgetReceipt, BudgetReceiptKind, ReservationPhase, ReservationReference,
};
use vstd::prelude::*;

verus! {

pub(in crate::transition) fn settle_exact(
    ledger: &mut BudgetLedger,
    reference: ReservationReference,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires crate::model::ledger_well_formed(old(ledger)),
    ensures
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                crate::BudgetCommand::SettleExact(reference),
                final(ledger),
                receipt,
            ),
            Err(error) => crate::reachability::rejection_cause(
                old(ledger),
                crate::BudgetCommand::SettleExact(reference),
                error,
            ),
        },
{
    let ghost before = *ledger;
    let result = finalize_full_ceiling(
        ledger,
        reference,
        BudgetOperation::SettleExact,
        ReservationPhase::SettledExact,
        crate::BudgetCommand::SettleExact(reference),
    );
    proof {
        if let Ok(receipt) = result {
            crate::reachability::settle_exact_refines(&before, ledger, reference, receipt);
        }
    }
    result
}

pub(in crate::transition) fn finalize_ambiguous(
    ledger: &mut BudgetLedger,
    finalization: AmbiguousFinalization,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires crate::model::ledger_well_formed(old(ledger)),
    ensures
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                crate::BudgetCommand::FinalizeAmbiguous(finalization),
                final(ledger),
                receipt,
            ),
            Err(error) => crate::reachability::rejection_cause(
                old(ledger),
                crate::BudgetCommand::FinalizeAmbiguous(finalization),
                error,
            ),
        },
{
    let ghost before = *ledger;
    let concrete_reference = finalization.verified_reference();
    let result = finalize_full_ceiling(
        ledger,
        concrete_reference,
        BudgetOperation::FinalizeAmbiguous,
        ReservationPhase::SettledAmbiguous,
        crate::BudgetCommand::FinalizeAmbiguous(finalization),
    );
    proof {
        if let Ok(receipt) = result {
            crate::reachability::ambiguous_finalization_refines(
                &before,
                ledger,
                finalization,
                concrete_reference,
                receipt,
            );
        }
    }
    result
}

fn finalize_full_ceiling(
    ledger: &mut BudgetLedger,
    reference: ReservationReference,
    operation: BudgetOperation,
    final_phase: ReservationPhase,
    command: crate::BudgetCommand,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires
        crate::model::ledger_well_formed(old(ledger)),
        match command {
            crate::BudgetCommand::SettleExact(command_reference) => {
                command_reference == reference
                    && operation == BudgetOperation::SettleExact
                    && final_phase == ReservationPhase::SettledExact
            }
            crate::BudgetCommand::FinalizeAmbiguous(finalization) => {
                finalization.spec_reference() == reference
                    && operation == BudgetOperation::FinalizeAmbiguous
                    && final_phase == ReservationPhase::SettledAmbiguous
            }
            _ => false,
        },
    ensures
        match result {
            Ok(receipt) => crate::reachability::full_finalization_exact(
                old(ledger),
                reference,
                operation,
                final_phase,
                final(ledger),
                receipt,
            ),
            Err(error) => crate::reachability::rejection_cause(
                old(ledger),
                command,
                error,
            ),
        },
{
    let _ = command;
    let ghost before = *ledger;
    let reservation_index = match require_reservation(
        ledger,
        reference.verified_reservation_id(),
    ) {
        Ok(index) => index,
        Err(error) => {
            assert(crate::reachability::rejection_cause(
                ledger,
                command,
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
                command,
                error,
            ));
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
        proof {
            crate::reachability::finalization_guard_from_runtime(
                ledger,
                reference,
                final_phase,
                BudgetReceiptKind::Idempotent,
                reservation_index as int,
            );
            assert(replay.spec_kind() == BudgetReceiptKind::Idempotent);
            prove_replayed_full_finalization(
                ledger, reference, operation, final_phase, budget_id, replay,
            );
        }
        return Ok(replay);
    }
    match record.phase {
        ReservationPhase::Active => {}
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
                command,
                error,
            ));
            return Err(error);
        }
    }
    let budget_id = record.request.budget_id();
    assert(crate::invariant::ledger_structure_holds(&before));
    assert(crate::invariant::reservation_entry_valid(
        &before,
        reservation_index as int,
    ));
    assert(before.reservations[reservation_index as int]
        .observed.spec_le(before.reservations[reservation_index as int].request.spec_reserve()));
    let remaining = outstanding_validated(record);
    BudgetAmounts::difference_le_left(
        remaining,
        record.request.reserve(),
        record.observed,
    );
    assert forall |dimension: crate::BudgetDimension| #![auto]
        remaining.spec_get(dimension)
            <= crate::accounting_model::record_outstanding(
                before.reservations[reservation_index as int],
                dimension,
            ) by {
        assert(remaining.spec_get(dimension)
            == crate::accounting_model::record_outstanding(
                before.reservations[reservation_index as int],
                dimension,
            ));
    }
    establish_reserved_charge_safe(ledger, reservation_index, remaining);
    release_full_reservation(ledger, reservation_index, remaining);
    let ghost released_state = *ledger;
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
    proof {
        assert(crate::reachability::reservation_bound_to_budget(
            &before,
            reference.spec_reservation_id(),
            budget_id,
        ));
        assert(before.reservations[reservation_index as int].phase
            == ReservationPhase::Active);
        assert(BudgetAmounts::spec_difference(
            remaining,
            before.reservations[reservation_index as int].request.spec_reserve(),
            before.reservations[reservation_index as int].observed,
        ));
        assert(crate::reachability::full_finalization_receipt_exact(
            applied_receipt,
            reference,
            operation,
            budget_id,
        ));
        assert(crate::reachability::reservation_accounting_exact(
            &before,
            ledger,
            &released_state,
            budget_id,
            remaining,
        ));
        assert(applied_receipt.spec_charged().spec_equal(remaining));
        assert(exists |exact_release_state: BudgetLedger, exact_charged: BudgetAmounts| #![auto]
            exact_charged.spec_equal(applied_receipt.spec_charged())
                && crate::reachability::reservation_accounting_exact(
                    &before,
                    ledger,
                    &exact_release_state,
                    budget_id,
                    exact_charged,
                ));
        assert(ledger.reservations@ == before.reservations@.update(
            reservation_index as int,
            ledger.reservations[reservation_index as int],
        ));
        assert(crate::reachability::full_finalization_record_exact(
            &before,
            ledger,
            reference,
            final_phase,
        ));
        assert(applied_receipt.spec_kind() == BudgetReceiptKind::Applied);
        crate::reachability::finalization_guard_from_runtime(
            &before,
            reference,
            final_phase,
            BudgetReceiptKind::Applied,
            reservation_index as int,
        );
        crate::reachability::full_finalization_refines(
            &before,
            ledger,
            reference,
            operation,
            final_phase,
            applied_receipt,
            budget_id,
        );
    }
    Ok(applied_receipt)
}

proof fn prove_replayed_full_finalization(
    ledger: &BudgetLedger,
    reference: ReservationReference,
    operation: BudgetOperation,
    final_phase: ReservationPhase,
    budget_id: peritus_types::BudgetId,
    replay_receipt: BudgetReceipt,
)
    requires crate::model::ledger_well_formed(ledger),
    crate::reachability::full_finalization_guard_exact(
        ledger,
        reference,
        final_phase,
        replay_receipt.spec_kind(),
    ),
    crate::reachability::reservation_bound_to_budget(
        ledger,
        reference.spec_reservation_id(),
        budget_id,
    ),
    crate::reachability::full_finalization_receipt_exact(
        replay_receipt,
        reference,
        operation,
        budget_id,
    ),
    replay_receipt.spec_kind() == BudgetReceiptKind::Idempotent,
    replay_receipt.spec_charged().spec_is_zero(),
    ensures crate::reachability::full_finalization_exact(
        ledger,
        reference,
        operation,
        final_phase,
        ledger,
        replay_receipt,
    ),
{
    crate::reachability::ledger_exact_reflexive(ledger);
    crate::reachability::full_finalization_refines(
        ledger,
        ledger,
        reference,
        operation,
        final_phase,
        replay_receipt,
        budget_id,
    );
}

} // verus!
