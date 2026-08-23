//! Functionality of exact full-ceiling finalization candidates.

#[cfg(verus_only)]
use crate::{
    BudgetCommand, BudgetLedger, BudgetOperation, BudgetReceipt, BudgetReceiptKind,
    ReservationPhase, ReservationReference,
};
use vstd::prelude::*;

verus! {

#[allow(
    clippy::too_many_arguments,
    reason = "The proof compares two explicit candidate tuples and the closed command binding"
)]
pub(in crate::reachability::functionality) proof fn full_finalization_candidates_equal(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    command: BudgetCommand,
    reference: ReservationReference,
    operation: BudgetOperation,
    phase: ReservationPhase,
    left_after: &BudgetLedger,
    left_receipt: BudgetReceipt,
    right_after: &BudgetLedger,
    right_receipt: BudgetReceipt,
)
    requires
        left_before.accounts@ == right_before.accounts@,
        left_before.reservations@ == right_before.reservations@,
        crate::reachability::raw_accepted_step(
            left_before, command, left_after, left_receipt,
        ),
        crate::reachability::raw_accepted_step(
            right_before, command, right_after, right_receipt,
        ),
        match command {
            BudgetCommand::SettleExact(actual) => {
                actual == reference
                    && operation == BudgetOperation::SettleExact
                    && phase == ReservationPhase::SettledExact
            }
            BudgetCommand::FinalizeAmbiguous(actual) => {
                actual.spec_reference() == reference
                    && operation == BudgetOperation::FinalizeAmbiguous
                    && phase == ReservationPhase::SettledAmbiguous
            }
            _ => false,
        },
    ensures
        crate::reachability::commands::ledger_views_equal(left_after, right_after),
        crate::reachability::commands::receipts_exactly_equal(left_receipt, right_receipt),
{
    reveal(crate::reachability::raw_accepted_step);
    reveal(crate::reachability::guards::accepted_command_guard);
    reveal(crate::reachability::lifecycle_steps::lifecycle_step);
    reveal(crate::reachability::lifecycle_steps::full_finalization_step);
    reveal(crate::reachability::lifecycle_steps::finalization_receipt);
    reveal(crate::reachability::commands::receipt_identity);
    reveal(crate::reachability::commands::receipts_exactly_equal);
    match command {
        BudgetCommand::SettleExact(actual) => {
            assert(actual == reference);
            assert(crate::reachability::lifecycle_steps::full_finalization_step(
                left_before, reference, operation, phase, left_after, left_receipt,
            ));
            assert(crate::reachability::lifecycle_steps::full_finalization_step(
                right_before, reference, operation, phase, right_after, right_receipt,
            ));
        }
        BudgetCommand::FinalizeAmbiguous(actual) => {
            assert(actual.spec_reference() == reference);
            assert(crate::reachability::lifecycle_steps::full_finalization_step(
                left_before, reference, operation, phase, left_after, left_receipt,
            ));
            assert(crate::reachability::lifecycle_steps::full_finalization_step(
                right_before, reference, operation, phase, right_after, right_receipt,
            ));
        }
        _ => assert(false),
    }
    super::finalization_kinds_equal(
        left_before, right_before, reference, phase,
        left_receipt.spec_kind(), right_receipt.spec_kind(),
    );
    let left_receipt_budget = choose |budget: peritus_types::BudgetId| #![auto]
        crate::reachability::commands::bound_budget(
            left_before, reference.spec_reservation_id(), budget,
        ) && crate::reachability::lifecycle_steps::finalization_receipt(
            left_receipt, operation, budget,
            reference.spec_reservation_id(), reference.spec_evidence_digest(),
        );
    let right_receipt_budget = choose |budget: peritus_types::BudgetId| #![auto]
        crate::reachability::commands::bound_budget(
            right_before, reference.spec_reservation_id(), budget,
        ) && crate::reachability::lifecycle_steps::finalization_receipt(
            right_receipt, operation, budget,
            reference.spec_reservation_id(), reference.spec_evidence_digest(),
        );
    super::bound_budgets_equal(
        left_before, right_before, reference.spec_reservation_id(),
        left_receipt_budget, right_receipt_budget,
    );
    crate::identity_model::budget_ids_transitive(
        left_receipt.spec_budget_id(), left_receipt_budget, right_receipt_budget,
    );
    crate::identity_model::budget_ids_symmetric(
        right_receipt.spec_budget_id(), right_receipt_budget,
    );
    crate::identity_model::budget_ids_transitive(
        left_receipt.spec_budget_id(), right_receipt_budget,
        right_receipt.spec_budget_id(),
    );
    match (left_receipt.spec_kind(), right_receipt.spec_kind()) {
        (BudgetReceiptKind::Idempotent, BudgetReceiptKind::Idempotent) => {}
        (BudgetReceiptKind::Applied, BudgetReceiptKind::Applied) => {
            let left_budget = choose |budget: peritus_types::BudgetId| #![auto]
                crate::reachability::commands::bound_budget(
                    left_before, reference.spec_reservation_id(), budget,
                ) && crate::reachability::lifecycle_steps::finalization_receipt(
                    left_receipt, operation, budget,
                    reference.spec_reservation_id(), reference.spec_evidence_digest(),
                ) && exists |released: BudgetLedger, exact: crate::BudgetAmounts| #![auto]
                    exact.spec_equal(left_receipt.spec_charged())
                        && crate::reachability::account_updates::full_charge_accounting(
                            left_before, left_after, &released, budget, exact,
                        );
            let right_budget = choose |budget: peritus_types::BudgetId| #![auto]
                crate::reachability::commands::bound_budget(
                    right_before, reference.spec_reservation_id(), budget,
                ) && crate::reachability::lifecycle_steps::finalization_receipt(
                    right_receipt, operation, budget,
                    reference.spec_reservation_id(), reference.spec_evidence_digest(),
                ) && exists |released: BudgetLedger, exact: crate::BudgetAmounts| #![auto]
                    exact.spec_equal(right_receipt.spec_charged())
                        && crate::reachability::account_updates::full_charge_accounting(
                            right_before, right_after, &released, budget, exact,
                        );
            super::bound_budgets_equal(
                left_before, right_before, reference.spec_reservation_id(),
                left_budget, right_budget,
            );
            let left_index = choose |index: int| #![auto]
                0 <= index < left_before.reservations@.len()
                    && crate::identity_model::reservation_ids_equal(
                        left_before.reservations[index].request.spec_reservation_id(),
                        reference.spec_reservation_id(),
                    )
                    && crate::BudgetAmounts::spec_difference(
                        left_receipt.spec_charged(),
                        left_before.reservations[index].request.spec_reserve(),
                        left_before.reservations[index].observed,
                    );
            let right_index = choose |index: int| #![auto]
                0 <= index < right_before.reservations@.len()
                    && crate::identity_model::reservation_ids_equal(
                        right_before.reservations[index].request.spec_reservation_id(),
                        reference.spec_reservation_id(),
                    )
                    && crate::BudgetAmounts::spec_difference(
                        right_receipt.spec_charged(),
                        right_before.reservations[index].request.spec_reserve(),
                        right_before.reservations[index].observed,
                    );
            crate::invariant::matching_reservations_are_unique(
                left_before, left_index, right_index,
            );
            assert(left_index == right_index);
            super::differences_are_unique(
                left_receipt.spec_charged(), right_receipt.spec_charged(),
                left_before.reservations[left_index].request.spec_reserve(),
                left_before.reservations[left_index].observed,
            );
            let left_exact = choose |amount: crate::BudgetAmounts| #![auto]
                amount.spec_equal(left_receipt.spec_charged())
                    && exists |released: BudgetLedger| #![auto]
                        crate::reachability::account_updates::full_charge_accounting(
                            left_before, left_after, &released, left_budget, amount,
                        );
            let right_exact = choose |amount: crate::BudgetAmounts| #![auto]
                amount.spec_equal(right_receipt.spec_charged())
                    && exists |released: BudgetLedger| #![auto]
                        crate::reachability::account_updates::full_charge_accounting(
                            right_before, right_after, &released, right_budget, amount,
                        );
            super::amounts_equal_through(
                left_exact, left_receipt.spec_charged(), right_exact,
            );
            assert(crate::model::ledger_well_formed(left_before));
            super::super::accounting::well_formed_has_unique_account_ids(left_before);
            super::super::release::full_charge_functional(
                left_before, right_before, left_after, right_after,
                left_budget, right_budget, left_exact, right_exact,
            );
            assert(crate::reachability::reservations::full_finalization_effect(
                left_before, right_after, reference.spec_reservation_id(),
                reference.spec_evidence_digest(), phase,
            ));
            super::super::reservation_updates::finalization_effects_equal(
                left_before, left_after, right_after,
                reference.spec_reservation_id(),
                reference.spec_evidence_digest(), phase,
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
    assert(crate::reachability::commands::receipts_exactly_equal(
        left_receipt, right_receipt,
    ));
}

} // verus!
