//! Exact functionality of accepted over-ceiling observation candidates.

#[cfg(verus_only)]
use crate::{
    BudgetLedger, BudgetReceipt, BudgetReceiptKind, ReservationPhase, UsageObservation,
};
#[cfg(verus_only)]
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

#[verifier::spinoff_prover]
#[allow(
    clippy::too_many_arguments,
    reason = "The refinement compares both explicit candidate tuples and their bound witnesses"
)]
pub(super) proof fn overrun_candidates_equal(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    observation: UsageObservation,
    left_after: &BudgetLedger,
    left_receipt: BudgetReceipt,
    right_after: &BudgetLedger,
    right_receipt: BudgetReceipt,
    left_index: int,
    right_index: int,
    left_budget: BudgetId,
    right_budget: BudgetId,
)
    requires
        crate::model::ledger_well_formed(left_before),
        crate::model::ledger_well_formed(right_before),
        left_before.accounts@ == right_before.accounts@,
        left_before.reservations@ == right_before.reservations@,
        crate::reachability::guards::reservation_at(
            left_before, observation.spec_reservation_id(), left_index,
        ),
        crate::reachability::guards::reservation_at(
            right_before, observation.spec_reservation_id(), right_index,
        ),
        left_index == right_index,
        crate::identity_model::budget_ids_equal(left_budget, right_budget),
        left_receipt.spec_kind() == BudgetReceiptKind::OverrunFaulted,
        right_receipt.spec_kind() == BudgetReceiptKind::OverrunFaulted,
        crate::reachability::commands::observation_budget_step(
            left_before, observation, left_after, left_receipt, left_budget,
        ),
        crate::reachability::commands::observation_budget_step(
            right_before, observation, right_after, right_receipt, right_budget,
        ),
    ensures
        crate::reachability::commands::ledger_views_equal(left_after, right_after),
        crate::reachability::commands::receipts_exactly_equal(left_receipt, right_receipt),
{
    reveal(crate::reachability::commands::observation_budget_step);
    reveal(crate::reachability::commands::observation_overrun_effect);
    let left_effect_index = choose |index: int| #![auto]
        crate::reachability::commands::observation_overrun_effect(
            left_before, observation, left_after, left_receipt, left_budget, index,
        );
    let right_effect_index = choose |index: int| #![auto]
        crate::reachability::commands::observation_overrun_effect(
            right_before, observation, right_after, right_receipt, right_budget, index,
        );
    crate::invariant::matching_reservations_are_unique(
        left_before, left_index, left_effect_index,
    );
    crate::invariant::matching_reservations_are_unique(
        left_before, left_index, right_effect_index,
    );
    assert(left_effect_index == left_index);
    assert(right_effect_index == left_index);
    match left_before.reservations[left_index].phase {
        ReservationPhase::SettledFinal | ReservationPhase::OverrunFaulted => {}
        ReservationPhase::Active => {
            super::super::finalization::differences_are_unique(
                left_receipt.spec_charged(),
                right_receipt.spec_charged(),
                left_before.reservations[left_index].request.spec_reserve(),
                left_before.reservations[left_index].observed,
            );
            reveal(super::overrun_witness::overrun_release_witness);
            reveal(super::overrun_witness::overrun_charged_witness);
            super::overrun_witness::overrun_effect_has_release(
                left_before,
                observation,
                left_after,
                left_receipt,
                left_budget,
                left_effect_index,
            );
            let left_released = choose |released: BudgetLedger| #![auto]
                super::overrun_witness::overrun_release_witness(
                    left_before, left_after, left_budget, left_receipt, released,
                );
            super::overrun_witness::overrun_release_has_charged(
                left_before, left_after, left_budget, left_receipt, &left_released,
            );
            let left_charged_state = choose |state: BudgetLedger| #![auto]
                super::overrun_witness::overrun_charged_witness(
                    left_before, left_after, left_budget, left_receipt, &left_released, state,
                );
            let left_exact = choose |exact: crate::BudgetAmounts| #![auto]
                exact.spec_equal(left_receipt.spec_charged())
                    && crate::reachability::account_updates::overrun_accounting(
                        left_before,
                        left_after,
                        &left_released,
                        &left_charged_state,
                        left_budget,
                        exact,
                    );
            super::overrun_witness::overrun_effect_has_release(
                right_before,
                observation,
                right_after,
                right_receipt,
                right_budget,
                right_effect_index,
            );
            let right_released = choose |released: BudgetLedger| #![auto]
                super::overrun_witness::overrun_release_witness(
                    right_before, right_after, right_budget, right_receipt, released,
                );
            super::overrun_witness::overrun_release_has_charged(
                right_before, right_after, right_budget, right_receipt, &right_released,
            );
            let right_charged_state = choose |state: BudgetLedger| #![auto]
                super::overrun_witness::overrun_charged_witness(
                    right_before,
                    right_after,
                    right_budget,
                    right_receipt,
                    &right_released,
                    state,
                );
            let right_exact = choose |exact: crate::BudgetAmounts| #![auto]
                exact.spec_equal(right_receipt.spec_charged())
                    && crate::reachability::account_updates::overrun_accounting(
                        right_before,
                        right_after,
                        &right_released,
                        &right_charged_state,
                        right_budget,
                        exact,
                    );
            super::super::finalization::amounts_equal_through(
                left_exact, left_receipt.spec_charged(), right_exact,
            );
            super::super::accounting::well_formed_has_unique_account_ids(left_before);
            super::super::fault::overrun_functional(
                left_before,
                right_before,
                left_after,
                right_after,
                &left_released,
                &right_released,
                &left_charged_state,
                &right_charged_state,
                left_budget,
                right_budget,
                left_exact,
                right_exact,
            );
            assert(crate::reachability::reservations::observation_effect(
                left_before,
                right_after,
                observation.spec_reservation_id(),
                left_before.reservations[left_index].request.spec_reserve(),
                observation.spec_evidence_digest(),
                ReservationPhase::OverrunFaulted,
                Some(observation.spec_cumulative()),
                Some(observation.spec_finality()),
            ));
            super::super::reservation_updates::observation_effects_equal(
                left_before,
                left_after,
                right_after,
                observation.spec_reservation_id(),
                left_before.reservations[left_index].request.spec_reserve(),
                observation.spec_evidence_digest(),
                ReservationPhase::OverrunFaulted,
                Some(observation.spec_cumulative()),
                Some(observation.spec_finality()),
            );
        }
        _ => assert(false),
    }
    assert(crate::reachability::commands::ledger_views_equal(left_after, right_after));
    assert(crate::reachability::commands::receipts_exactly_equal(
        left_receipt, right_receipt,
    ));
}

} // verus!
