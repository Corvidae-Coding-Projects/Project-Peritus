//! Command-local exact-effect predicates and reducer refinement lemmas.

#[cfg(verus_only)]
use crate::{BudgetCommand, BudgetLedger, BudgetReceipt};
use vstd::prelude::*;

mod observation;

#[cfg(verus_only)]
pub(crate) use observation::{
    observation_receipt_exact, observation_record_exact, observation_refines,
    overrun_active_effect_exact, overrun_terminal_effect_exact, reservation_bound_to_budget,
};

verus! {

pub(crate) open spec fn full_finalization_exact(
    before: &BudgetLedger,
    reference: crate::ReservationReference,
    operation: crate::BudgetOperation,
    phase: crate::ReservationPhase,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
) -> bool {
    crate::model::ledger_well_formed(before)
        && super::guards::full_finalization_guard(before, reference, phase, receipt.spec_kind())
        && super::lifecycle_steps::full_finalization_step(
            before,
            reference,
            operation,
            phase,
            after,
            receipt,
        )
}

pub(crate) open spec fn full_finalization_receipt_exact(
    receipt: BudgetReceipt,
    reference: crate::ReservationReference,
    operation: crate::BudgetOperation,
    budget_id: peritus_types::BudgetId,
) -> bool {
    super::lifecycle_steps::finalization_receipt(
        receipt,
        operation,
        budget_id,
        reference.spec_reservation_id(),
        reference.spec_evidence_digest(),
    )
}

pub(crate) open spec fn full_finalization_record_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    reference: crate::ReservationReference,
    phase: crate::ReservationPhase,
) -> bool {
    super::reservations::full_finalization_effect(
        before,
        after,
        reference.spec_reservation_id(),
        reference.spec_evidence_digest(),
        phase,
    )
}

pub(crate) proof fn full_finalization_refines(
    before: &BudgetLedger,
    after: &BudgetLedger,
    reference: crate::ReservationReference,
    operation: crate::BudgetOperation,
    phase: crate::ReservationPhase,
    receipt: BudgetReceipt,
    budget_id: peritus_types::BudgetId,
)
    requires
        crate::model::ledger_well_formed(before),
        super::guards::full_finalization_guard(before, reference, phase, receipt.spec_kind()),
        reservation_bound_to_budget(before, reference.spec_reservation_id(), budget_id),
        full_finalization_receipt_exact(receipt, reference, operation, budget_id),
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied
            || receipt.spec_kind() == crate::BudgetReceiptKind::Idempotent,
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied
            ==> (exists |index: int| #![auto]
                0 <= index < before.reservations@.len()
                    && crate::identity_model::reservation_ids_equal(
                        before.reservations[index].request.spec_reservation_id(),
                        reference.spec_reservation_id(),
                    )
                    && crate::BudgetAmounts::spec_difference(
                        receipt.spec_charged(),
                        before.reservations[index].request.spec_reserve(),
                        before.reservations[index].observed,
                    ))
                && (exists |released_state: BudgetLedger,
                    exact_charged: crate::BudgetAmounts| #![auto]
                    exact_charged.spec_equal(receipt.spec_charged())
                        && super::account_updates::full_charge_accounting(
                        before,
                        after,
                        &released_state,
                        budget_id,
                        exact_charged,
                    ))
                && full_finalization_record_exact(before, after, reference, phase),
        receipt.spec_kind() == crate::BudgetReceiptKind::Idempotent
            ==> receipt.spec_charged().spec_is_zero()
                && super::ledgers_exactly_equal(before, after),
    ensures
        full_finalization_exact(before, reference, operation, phase, after, receipt),
{
    if receipt.spec_kind() == crate::BudgetReceiptKind::Applied {
        let index = choose |index: int| #![auto]
            0 <= index < before.reservations@.len()
                && crate::identity_model::reservation_ids_equal(
                    before.reservations[index].request.spec_reservation_id(),
                    reference.spec_reservation_id(),
                )
                && crate::BudgetAmounts::spec_difference(
                    receipt.spec_charged(),
                    before.reservations[index].request.spec_reserve(),
                    before.reservations[index].observed,
                );
        assert(exists |witness: int| #![auto]
            0 <= witness < before.reservations@.len()
                && crate::identity_model::reservation_ids_equal(
                    before.reservations[witness].request.spec_reservation_id(),
                    reference.spec_reservation_id(),
                )
                && crate::BudgetAmounts::spec_difference(
                    receipt.spec_charged(),
                    before.reservations[witness].request.spec_reserve(),
                    before.reservations[witness].observed,
                ));
    }
    assert(super::lifecycle_steps::full_finalization_step(
        before,
        reference,
        operation,
        phase,
        after,
        receipt,
    ));
}

pub(crate) proof fn settle_exact_refines(
    before: &BudgetLedger,
    after: &BudgetLedger,
    reference: crate::ReservationReference,
    receipt: BudgetReceipt,
)
    requires
        full_finalization_exact(
            before,
            reference,
            crate::BudgetOperation::SettleExact,
            crate::ReservationPhase::SettledExact,
            after,
            receipt,
        ),
    ensures
        super::candidate_step(
            before,
            BudgetCommand::SettleExact(reference),
            after,
            receipt,
        ),
{
    super::raw_step_is_accepted(
        before,
        BudgetCommand::SettleExact(reference),
        after,
        receipt,
    );
}

pub(crate) proof fn ambiguous_finalization_refines(
    before: &BudgetLedger,
    after: &BudgetLedger,
    finalization: crate::AmbiguousFinalization,
    concrete_reference: crate::ReservationReference,
    receipt: BudgetReceipt,
)
    requires
        concrete_reference == finalization.spec_reference(),
        crate::identity_model::reservation_ids_equal(
            concrete_reference.spec_reservation_id(),
            finalization.spec_reference().spec_reservation_id(),
        ),
        crate::identity_model::action_ids_equal(
            concrete_reference.spec_action_id(),
            finalization.spec_reference().spec_action_id(),
        ),
        crate::identity_model::digests_equal(
            concrete_reference.spec_action_digest(),
            finalization.spec_reference().spec_action_digest(),
        ),
        crate::identity_model::digests_equal(
            concrete_reference.spec_evidence_digest(),
            finalization.spec_reference().spec_evidence_digest(),
        ),
        full_finalization_exact(
            before,
            concrete_reference,
            crate::BudgetOperation::FinalizeAmbiguous,
            crate::ReservationPhase::SettledAmbiguous,
            after,
            receipt,
        ),
    ensures
        super::candidate_step(
            before,
            BudgetCommand::FinalizeAmbiguous(finalization),
            after,
            receipt,
        ),
{
    super::raw_step_is_accepted(
        before,
        BudgetCommand::FinalizeAmbiguous(finalization),
        after,
        receipt,
    );
}

pub(crate) open spec fn begin_receipt_exact(
    receipt: BudgetReceipt,
    request: crate::BudgetRequest,
) -> bool {
    super::commands::receipt_identity(
        receipt,
        crate::BudgetOperation::Begin,
        request.spec_budget_id(),
        Some(request.spec_reservation_id()),
    )
        && receipt.spec_released().spec_is_zero()
        && super::commands::receipt_has_no_observation(receipt)
}

pub(crate) open spec fn begin_record_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    request: crate::BudgetRequest,
) -> bool {
    super::reservations::begin_record_effect(before, after, request)
}

pub(crate) proof fn begin_refines(
    before: &BudgetLedger,
    after: &BudgetLedger,
    request: crate::BudgetRequest,
    receipt: BudgetReceipt,
)
    requires
        super::guards::accepted_command_guard(
            before,
            BudgetCommand::Begin(request),
            receipt.spec_kind(),
        ),
        begin_receipt_exact(receipt, request),
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied
            || receipt.spec_kind() == crate::BudgetReceiptKind::Idempotent,
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied
            ==> receipt.spec_charged().spec_equal(request.spec_consume_now())
                && (exists |charged_state: BudgetLedger| #![auto]
                    super::account_updates::begin_accounting(
                        before,
                        after,
                        &charged_state,
                        request.spec_budget_id(),
                        request.spec_consume_now(),
                        request.spec_reserve(),
                    ))
                && begin_record_exact(before, after, request),
        receipt.spec_kind() == crate::BudgetReceiptKind::Idempotent
            ==> receipt.spec_charged().spec_is_zero()
                && super::ledgers_exactly_equal(before, after),
    ensures
        super::candidate_step(before, BudgetCommand::Begin(request), after, receipt),
{
    super::raw_step_is_accepted(before, BudgetCommand::Begin(request), after, receipt);
}

} // verus!
