//! Closed per-command dispatch for candidate functionality.

#[cfg(verus_only)]
use crate::{BudgetCommand, BudgetLedger, BudgetReceipt};
use vstd::prelude::*;

verus! {

/// Every exact command candidate denotes the same stable ledger and receipt projections.
pub(crate) proof fn candidate_step_is_functional(
    before: &BudgetLedger,
    command: BudgetCommand,
    left_after: &BudgetLedger,
    left_receipt: BudgetReceipt,
    right_after: &BudgetLedger,
    right_receipt: BudgetReceipt,
)
    requires
        crate::reachability::candidate_step(before, command, left_after, left_receipt),
        crate::reachability::candidate_step(before, command, right_after, right_receipt),
    ensures
        crate::reachability::commands::ledger_views_equal(left_after, right_after),
        crate::reachability::commands::receipts_exactly_equal(left_receipt, right_receipt),
{
    let left_before = choose |logical_before: BudgetLedger| #![auto]
        crate::reachability::exact_input_view(before, &logical_before)
            && crate::reachability::raw_accepted_step(
                &logical_before, command, left_after, left_receipt,
            );
    let right_before = choose |logical_before: BudgetLedger| #![auto]
        crate::reachability::exact_input_view(before, &logical_before)
            && crate::reachability::raw_accepted_step(
                &logical_before, command, right_after, right_receipt,
            );
    assert(crate::reachability::raw_accepted_step(
        &left_before, command, left_after, left_receipt,
    ));
    assert(crate::reachability::raw_accepted_step(
        &right_before, command, right_after, right_receipt,
    ));
    assert(left_before.accounts@ == right_before.accounts@);
    assert(left_before.reservations@ == right_before.reservations@);
    match command {
        BudgetCommand::AllocateChild(request) => {
            super::allocation::allocation_candidates_equal(
                &left_before, &right_before, request,
                left_after, left_receipt, right_after, right_receipt,
            );
        }
        BudgetCommand::Begin(request) => {
            super::operations::begin_candidates_equal(
                &left_before, &right_before, request,
                left_after, left_receipt, right_after, right_receipt,
            );
        }
        BudgetCommand::Activate(activation) => {
            super::activation_candidates_equal(
                &left_before, &right_before, activation,
                left_after, left_receipt, right_after, right_receipt,
            );
        }
        BudgetCommand::ObserveUsage(observation) => {
            super::observation::observation_candidates_equal(
                &left_before, &right_before, observation,
                left_after, left_receipt, right_after, right_receipt,
            );
        }
        BudgetCommand::SettleExact(reference) => {
            super::finalization::full_finalization_candidates_equal(
                &left_before,
                &right_before,
                BudgetCommand::SettleExact(reference),
                reference,
                crate::BudgetOperation::SettleExact,
                crate::ReservationPhase::SettledExact,
                left_after,
                left_receipt,
                right_after,
                right_receipt,
            );
        }
        BudgetCommand::CancelHeld(reference) => {
            super::finalization::cancellation_candidates_equal(
                &left_before, &right_before, reference,
                left_after, left_receipt, right_after, right_receipt,
            );
        }
        BudgetCommand::FinalizeAmbiguous(finalization) => {
            super::finalization::full_finalization_candidates_equal(
                &left_before,
                &right_before,
                BudgetCommand::FinalizeAmbiguous(finalization),
                finalization.spec_reference(),
                crate::BudgetOperation::FinalizeAmbiguous,
                crate::ReservationPhase::SettledAmbiguous,
                left_after,
                left_receipt,
                right_after,
                right_receipt,
            );
        }
        BudgetCommand::Seal(budget_id) => {
            super::lifecycle::seal_candidates_equal(
                &left_before, &right_before, budget_id,
                left_after, left_receipt, right_after, right_receipt,
            );
        }
        BudgetCommand::Close(budget_id) => {
            super::lifecycle::close_candidates_equal(
                &left_before, &right_before, budget_id,
                left_after, left_receipt, right_after, right_receipt,
            );
        }
    }
    assert(crate::reachability::commands::ledger_views_equal(left_after, right_after));
    assert(crate::reachability::commands::receipts_exactly_equal(
        left_receipt, right_receipt,
    ));
}

} // verus!
