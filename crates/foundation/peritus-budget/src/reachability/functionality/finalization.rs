//! Functionality of reservation terminalization candidates.

#[cfg(verus_only)]
use crate::{
    BudgetCommand, BudgetLedger, BudgetOperation, BudgetReceipt, BudgetReceiptKind,
    ReservationPhase, ReservationReference,
};
use vstd::prelude::*;

mod full;

#[cfg(verus_only)]
pub(super) use full::full_finalization_candidates_equal;

verus! {

pub(super) proof fn differences_are_unique(
    left_result: crate::BudgetAmounts,
    right_result: crate::BudgetAmounts,
    minuend: crate::BudgetAmounts,
    subtrahend: crate::BudgetAmounts,
)
    requires
        crate::BudgetAmounts::spec_difference(left_result, minuend, subtrahend),
        crate::BudgetAmounts::spec_difference(right_result, minuend, subtrahend),
    ensures left_result.spec_equal(right_result),
{
}

pub(super) proof fn amounts_equal_through(
    left: crate::BudgetAmounts,
    middle: crate::BudgetAmounts,
    right: crate::BudgetAmounts,
)
    requires left.spec_equal(middle), right.spec_equal(middle),
    ensures left.spec_equal(right),
{
}

pub(super) proof fn bound_budgets_equal(
    left: &BudgetLedger,
    right: &BudgetLedger,
    reservation_id: peritus_types::BudgetReservationId,
    left_budget: peritus_types::BudgetId,
    right_budget: peritus_types::BudgetId,
)
    requires
        crate::model::ledger_well_formed(left),
        left.reservations@ == right.reservations@,
        super::super::commands::bound_budget(left, reservation_id, left_budget),
        super::super::commands::bound_budget(right, reservation_id, right_budget),
    ensures crate::identity_model::budget_ids_equal(left_budget, right_budget),
{
    reveal(super::super::commands::bound_budget);
    let left_index = choose |index: int| #![auto]
        0 <= index < left.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                left.reservations[index].request.spec_reservation_id(), reservation_id,
            )
            && crate::identity_model::budget_ids_equal(
                left.reservations[index].request.spec_budget_id(), left_budget,
            );
    let right_index = choose |index: int| #![auto]
        0 <= index < right.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                right.reservations[index].request.spec_reservation_id(), reservation_id,
            )
            && crate::identity_model::budget_ids_equal(
                right.reservations[index].request.spec_budget_id(), right_budget,
            );
    crate::invariant::matching_reservations_are_unique(left, left_index, right_index);
    assert(left_index == right_index);
    crate::identity_model::budget_ids_symmetric(
        left.reservations[left_index].request.spec_budget_id(), left_budget,
    );
    crate::identity_model::budget_ids_transitive(
        left_budget,
        left.reservations[left_index].request.spec_budget_id(),
        right_budget,
    );
}

proof fn finalization_kinds_equal(
    left: &BudgetLedger,
    right: &BudgetLedger,
    reference: ReservationReference,
    phase: ReservationPhase,
    left_kind: BudgetReceiptKind,
    right_kind: BudgetReceiptKind,
)
    requires
        crate::model::ledger_well_formed(left),
        phase == ReservationPhase::SettledExact
            || phase == ReservationPhase::SettledAmbiguous,
        left.reservations@ == right.reservations@,
        super::super::guards::full_finalization_guard(
            left, reference, phase, left_kind,
        ),
        super::super::guards::full_finalization_guard(
            right, reference, phase, right_kind,
        ),
    ensures left_kind == right_kind,
{
    reveal(super::super::guards::full_finalization_guard);
    let left_index = choose |index: int| #![auto]
        super::super::guards::reservation_at(
            left, reference.spec_reservation_id(), index,
        ) && match left_kind {
            BudgetReceiptKind::Applied => {
                left.reservations[index].phase == ReservationPhase::Active
            }
            BudgetReceiptKind::Idempotent => {
                left.reservations[index].phase == phase
            }
            BudgetReceiptKind::OverrunFaulted => false,
        };
    let right_index = choose |index: int| #![auto]
        super::super::guards::reservation_at(
            right, reference.spec_reservation_id(), index,
        ) && match right_kind {
            BudgetReceiptKind::Applied => {
                right.reservations[index].phase == ReservationPhase::Active
            }
            BudgetReceiptKind::Idempotent => {
                right.reservations[index].phase == phase
            }
            BudgetReceiptKind::OverrunFaulted => false,
        };
    crate::invariant::matching_reservations_are_unique(left, left_index, right_index);
    assert(left_index == right_index);
}

proof fn cancellation_kinds_equal(
    left: &BudgetLedger,
    right: &BudgetLedger,
    reference: ReservationReference,
    left_kind: BudgetReceiptKind,
    right_kind: BudgetReceiptKind,
)
    requires
        crate::model::ledger_well_formed(left),
        left.reservations@ == right.reservations@,
        super::super::guards::cancellation_guard(left, reference, left_kind),
        super::super::guards::cancellation_guard(right, reference, right_kind),
    ensures left_kind == right_kind,
{
    reveal(super::super::guards::cancellation_guard);
    let left_index = choose |index: int| #![auto]
        super::super::guards::reservation_at(
            left, reference.spec_reservation_id(), index,
        ) && match left_kind {
            BudgetReceiptKind::Applied => {
                left.reservations[index].phase == ReservationPhase::Held
            }
            BudgetReceiptKind::Idempotent => {
                left.reservations[index].phase == ReservationPhase::CancelledHeld
            }
            BudgetReceiptKind::OverrunFaulted => false,
        };
    let right_index = choose |index: int| #![auto]
        super::super::guards::reservation_at(
            right, reference.spec_reservation_id(), index,
        ) && match right_kind {
            BudgetReceiptKind::Applied => {
                right.reservations[index].phase == ReservationPhase::Held
            }
            BudgetReceiptKind::Idempotent => {
                right.reservations[index].phase == ReservationPhase::CancelledHeld
            }
            BudgetReceiptKind::OverrunFaulted => false,
        };
    crate::invariant::matching_reservations_are_unique(left, left_index, right_index);
    assert(left_index == right_index);
}

pub(super) proof fn cancellation_candidates_equal(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    reference: ReservationReference,
    left_after: &BudgetLedger,
    left_receipt: BudgetReceipt,
    right_after: &BudgetLedger,
    right_receipt: BudgetReceipt,
)
    requires
        left_before.accounts@ == right_before.accounts@,
        left_before.reservations@ == right_before.reservations@,
        super::super::raw_accepted_step(
            left_before, BudgetCommand::CancelHeld(reference), left_after, left_receipt,
        ),
        super::super::raw_accepted_step(
            right_before, BudgetCommand::CancelHeld(reference), right_after, right_receipt,
        ),
    ensures
        super::super::commands::ledger_views_equal(left_after, right_after),
        super::super::commands::receipts_exactly_equal(left_receipt, right_receipt),
{
    reveal(super::super::raw_accepted_step);
    reveal(super::super::guards::accepted_command_guard);
    reveal(super::super::lifecycle_steps::lifecycle_step);
    reveal(super::super::lifecycle_steps::cancel_held_step);
    reveal(super::super::commands::receipt_identity);
    reveal(super::super::commands::receipts_exactly_equal);
    assert(super::super::lifecycle_steps::cancel_held_step(
        left_before, reference, left_after, left_receipt,
    ));
    assert(super::super::lifecycle_steps::cancel_held_step(
        right_before, reference, right_after, right_receipt,
    ));
    cancellation_kinds_equal(
        left_before,
        right_before,
        reference,
        left_receipt.spec_kind(),
        right_receipt.spec_kind(),
    );
    let left_receipt_budget = choose |budget: peritus_types::BudgetId| #![auto]
        super::super::commands::bound_budget(
            left_before, reference.spec_reservation_id(), budget,
        ) && super::super::commands::receipt_identity(
            left_receipt, BudgetOperation::CancelHeld, budget,
            Some(reference.spec_reservation_id()),
        );
    let right_receipt_budget = choose |budget: peritus_types::BudgetId| #![auto]
        super::super::commands::bound_budget(
            right_before, reference.spec_reservation_id(), budget,
        ) && super::super::commands::receipt_identity(
            right_receipt, BudgetOperation::CancelHeld, budget,
            Some(reference.spec_reservation_id()),
        );
    bound_budgets_equal(
        left_before, right_before, reference.spec_reservation_id(),
        left_receipt_budget, right_receipt_budget,
    );
    crate::identity_model::budget_ids_transitive(
        left_receipt.spec_budget_id(),
        left_receipt_budget,
        right_receipt_budget,
    );
    crate::identity_model::budget_ids_symmetric(
        right_receipt.spec_budget_id(),
        right_receipt_budget,
    );
    crate::identity_model::budget_ids_transitive(
        left_receipt.spec_budget_id(),
        right_receipt_budget,
        right_receipt.spec_budget_id(),
    );
    match (left_receipt.spec_kind(), right_receipt.spec_kind()) {
        (BudgetReceiptKind::Idempotent, BudgetReceiptKind::Idempotent) => {}
        (BudgetReceiptKind::Applied, BudgetReceiptKind::Applied) => {
            let left_budget = choose |budget: peritus_types::BudgetId| #![auto]
                super::super::commands::bound_budget(
                    left_before, reference.spec_reservation_id(), budget,
                ) && exists |exact: crate::BudgetAmounts| #![auto]
                    exact.spec_equal(left_receipt.spec_released())
                        && super::super::account_updates::operation_release(
                            left_before, left_after, budget, exact,
                        );
            let right_budget = choose |budget: peritus_types::BudgetId| #![auto]
                super::super::commands::bound_budget(
                    right_before, reference.spec_reservation_id(), budget,
                ) && exists |exact: crate::BudgetAmounts| #![auto]
                    exact.spec_equal(right_receipt.spec_released())
                        && super::super::account_updates::operation_release(
                            right_before, right_after, budget, exact,
                        );
            bound_budgets_equal(
                left_before, right_before, reference.spec_reservation_id(),
                left_budget, right_budget,
            );
            let left_index = choose |index: int| #![auto]
                0 <= index < left_before.reservations@.len()
                    && crate::identity_model::reservation_ids_equal(
                        left_before.reservations[index].request.spec_reservation_id(),
                        reference.spec_reservation_id(),
                    )
                    && left_receipt.spec_released().spec_equal(
                        left_before.reservations[index].request.spec_reserve(),
                    );
            let right_index = choose |index: int| #![auto]
                0 <= index < right_before.reservations@.len()
                    && crate::identity_model::reservation_ids_equal(
                        right_before.reservations[index].request.spec_reservation_id(),
                        reference.spec_reservation_id(),
                    )
                    && right_receipt.spec_released().spec_equal(
                        right_before.reservations[index].request.spec_reserve(),
                    );
            crate::invariant::matching_reservations_are_unique(
                left_before, left_index, right_index,
            );
            assert(left_index == right_index);
            assert(left_receipt.spec_released().spec_equal(
                right_receipt.spec_released(),
            ));
            let left_exact = choose |amount: crate::BudgetAmounts| #![auto]
                amount.spec_equal(left_receipt.spec_released())
                    && super::super::account_updates::operation_release(
                        left_before, left_after, left_budget, amount,
                    );
            let right_exact = choose |amount: crate::BudgetAmounts| #![auto]
                amount.spec_equal(right_receipt.spec_released())
                    && super::super::account_updates::operation_release(
                        right_before, right_after, right_budget, amount,
                    );
            amounts_equal_through(
                left_exact, left_receipt.spec_released(), right_exact,
            );
            assert(crate::model::ledger_well_formed(left_before));
            super::accounting::well_formed_has_unique_account_ids(left_before);
            super::release::operation_release_functional(
                left_before, right_before, left_after, right_after,
                left_budget, right_budget,
                left_exact, right_exact,
            );
            assert(super::super::reservations::cancellation_effect(
                left_before, right_after, reference.spec_reservation_id(),
                reference.spec_evidence_digest(),
            ));
            super::reservation_updates::cancellation_effects_equal(
                left_before, left_after, right_after,
                reference.spec_reservation_id(), reference.spec_evidence_digest(),
            );
        }
        (BudgetReceiptKind::Idempotent, BudgetReceiptKind::Applied)
        | (BudgetReceiptKind::Applied, BudgetReceiptKind::Idempotent) => assert(false),
        _ => assert(false),
    }
    assert(left_receipt.spec_kind() == right_receipt.spec_kind());
    assert(left_receipt.spec_operation() == right_receipt.spec_operation());
    assert(crate::identity_model::budget_ids_equal(
        left_receipt.spec_budget_id(), right_receipt.spec_budget_id(),
    ));
    assert(crate::state::optional_reservation_ids_equal(
        left_receipt.spec_reservation_id(), right_receipt.spec_reservation_id(),
    ));
    assert(left_receipt.spec_charged().spec_equal(right_receipt.spec_charged()));
    assert(left_receipt.spec_released().spec_equal(right_receipt.spec_released()));
    assert(crate::invariant::optional_amounts_equal(
        left_receipt.spec_reported(), right_receipt.spec_reported(),
    ));
    assert(crate::invariant::optional_digests_equal(
        left_receipt.spec_evidence_digest(), right_receipt.spec_evidence_digest(),
    ));
    assert(super::super::commands::receipts_exactly_equal(
        left_receipt, right_receipt,
    ));
}

} // verus!
