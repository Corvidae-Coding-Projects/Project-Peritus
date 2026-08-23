//! Exact functionality of accepted in-ceiling high-water observations.

#[cfg(verus_only)]
use crate::{
    BudgetLedger, BudgetReceipt, BudgetReceiptKind, ReservationPhase, UsageFinality,
    UsageObservation,
};
#[cfg(verus_only)]
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

#[allow(
    clippy::too_many_arguments,
    reason = "The refinement compares both explicit candidate tuples and their bound witnesses"
)]
pub(super) proof fn applied_candidates_equal(
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
        crate::identity_model::budget_ids_equal(left_budget, right_budget),
        crate::reachability::guards::reservation_at(
            left_before, observation.spec_reservation_id(), left_index,
        ),
        crate::reachability::guards::reservation_at(
            right_before, observation.spec_reservation_id(), right_index,
        ),
        left_index == right_index,
        left_receipt.spec_kind() == BudgetReceiptKind::Applied,
        right_receipt.spec_kind() == BudgetReceiptKind::Applied,
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
    reveal(crate::reachability::commands::observation_applied_delta);
    let left_delta_index = choose |index: int| #![auto]
        crate::reachability::commands::observation_applied_delta(
            left_before, observation, left_receipt, index,
        );
    let right_delta_index = choose |index: int| #![auto]
        crate::reachability::commands::observation_applied_delta(
            right_before, observation, right_receipt, index,
        );
    crate::invariant::matching_reservations_are_unique(
        left_before, left_index, left_delta_index,
    );
    crate::invariant::matching_reservations_are_unique(
        left_before, left_index, right_delta_index,
    );
    assert(left_delta_index == left_index);
    assert(right_delta_index == left_index);
    super::super::finalization::differences_are_unique(
        left_receipt.spec_charged(), right_receipt.spec_charged(),
        observation.spec_cumulative(), left_before.reservations[left_index].observed,
    );
    if observation.spec_finality() == UsageFinality::Final {
        super::super::finalization::differences_are_unique(
            left_receipt.spec_released(), right_receipt.spec_released(),
            left_before.reservations[left_index].request.spec_reserve(),
            observation.spec_cumulative(),
        );
    }
    let left_exact_charged = choose |amount: crate::BudgetAmounts| #![auto]
        amount.spec_equal(left_receipt.spec_charged())
            && exists |released_state: BudgetLedger,
                exact_released: crate::BudgetAmounts| #![auto]
                exact_released.spec_equal(left_receipt.spec_released())
                    && crate::reachability::account_updates::reservation_accounting(
                        left_before, left_after, &released_state,
                        left_budget, amount, exact_released,
                    );
    let left_exact_released = choose |amount: crate::BudgetAmounts| #![auto]
        amount.spec_equal(left_receipt.spec_released())
            && exists |released_state: BudgetLedger| #![auto]
                crate::reachability::account_updates::reservation_accounting(
                    left_before, left_after, &released_state,
                    left_budget, left_exact_charged, amount,
                );
    let right_exact_charged = choose |amount: crate::BudgetAmounts| #![auto]
        amount.spec_equal(right_receipt.spec_charged())
            && exists |released_state: BudgetLedger,
                exact_released: crate::BudgetAmounts| #![auto]
                exact_released.spec_equal(right_receipt.spec_released())
                    && crate::reachability::account_updates::reservation_accounting(
                        right_before, right_after, &released_state,
                        right_budget, amount, exact_released,
                    );
    let right_exact_released = choose |amount: crate::BudgetAmounts| #![auto]
        amount.spec_equal(right_receipt.spec_released())
            && exists |released_state: BudgetLedger| #![auto]
                crate::reachability::account_updates::reservation_accounting(
                    right_before, right_after, &released_state,
                    right_budget, right_exact_charged, amount,
                );
    super::super::finalization::amounts_equal_through(
        left_exact_charged, left_receipt.spec_charged(), right_exact_charged,
    );
    super::super::finalization::amounts_equal_through(
        left_exact_released, left_receipt.spec_released(), right_exact_released,
    );
    super::super::accounting::well_formed_has_unique_account_ids(left_before);
    super::super::release::reservation_accounting_functional(
        left_before, right_before, left_after, right_after,
        left_budget, right_budget,
        left_exact_charged, right_exact_charged,
        left_exact_released, right_exact_released,
    );
    let phase = if observation.spec_finality() == UsageFinality::Final {
        ReservationPhase::SettledFinal
    } else {
        ReservationPhase::Active
    };
    let final_reported = if observation.spec_finality() == UsageFinality::Final {
        Some(observation.spec_cumulative())
    } else {
        None
    };
    let finality = if observation.spec_finality() == UsageFinality::Final {
        Some(UsageFinality::Final)
    } else {
        None
    };
    assert(crate::reachability::reservations::observation_effect(
        left_before, right_after, observation.spec_reservation_id(),
        observation.spec_cumulative(), observation.spec_evidence_digest(),
        phase, final_reported, finality,
    ));
    super::super::reservation_updates::observation_effects_equal(
        left_before, left_after, right_after,
        observation.spec_reservation_id(), observation.spec_cumulative(),
        observation.spec_evidence_digest(), phase, final_reported, finality,
    );
    assert(crate::reachability::commands::ledger_views_equal(left_after, right_after));
    assert(crate::reachability::commands::receipts_exactly_equal(
        left_receipt, right_receipt,
    ));
}

} // verus!
