//! Proof-only bridges from accounting primitives to exact observation steps.

#[cfg(verus_only)]
use crate::{BudgetAmounts, BudgetLedger, BudgetReceipt, ReservationPhase, UsageFinality, UsageObservation};
use vstd::prelude::*;

verus! {

pub(super) proof fn final_observation_refines(
    before: &BudgetLedger,
    after: &BudgetLedger,
    released_state: &BudgetLedger,
    charged_state: &BudgetLedger,
    observation: UsageObservation,
    receipt: BudgetReceipt,
    budget_id: peritus_types::BudgetId,
    charged: BudgetAmounts,
    released: BudgetAmounts,
)
    requires
        crate::reachability::accepted_guard(
            before,
            crate::BudgetCommand::ObserveUsage(observation),
            receipt.spec_kind(),
        ),
        crate::reachability::reservation_bound_to_budget(
            before,
            observation.spec_reservation_id(),
            budget_id,
        ),
        crate::reachability::observation_receipt_exact(receipt, observation, budget_id),
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied,
        observation.spec_finality() == UsageFinality::Final,
        crate::reachability::operation_release_exact(
            before,
            released_state,
            budget_id,
            charged,
        ),
        crate::reachability::lineage_charge_exact(
            released_state,
            charged_state,
            budget_id,
            charged,
        ),
        crate::reachability::operation_release_exact(
            charged_state,
            after,
            budget_id,
            released,
        ),
        charged.spec_equal(receipt.spec_charged()),
        released.spec_equal(receipt.spec_released()),
        exists |index: int| #![auto]
            0 <= index < before.reservations@.len()
                && crate::identity_model::reservation_ids_equal(
                    before.reservations[index].request.spec_reservation_id(),
                    observation.spec_reservation_id(),
                )
                && BudgetAmounts::spec_difference(
                    charged,
                    observation.spec_cumulative(),
                    before.reservations[index].observed,
                )
                && BudgetAmounts::spec_difference(
                    released,
                    before.reservations[index].request.spec_reserve(),
                    observation.spec_cumulative(),
                ),
        crate::reachability::observation_record_exact(
            before,
            after,
            observation,
            observation.spec_cumulative(),
            ReservationPhase::SettledFinal,
            Some(observation.spec_cumulative()),
            Some(UsageFinality::Final),
        ),
    ensures crate::reachability::candidate_step(
        before,
        crate::BudgetCommand::ObserveUsage(observation),
        after,
        receipt,
    ),
{
    assert(crate::reachability::observation_accounting_exact(
        before,
        after,
        released_state,
        budget_id,
        charged,
        released,
    )) by {
        assert(exists |exact_charged_state: BudgetLedger| #![auto]
            crate::reachability::lineage_charge_exact(
                released_state,
                &exact_charged_state,
                budget_id,
                charged,
            ) && crate::reachability::operation_release_exact(
                &exact_charged_state,
                after,
                budget_id,
                released,
            ));
    }
    assert(exists |exact_release_state: BudgetLedger, exact_charged: BudgetAmounts,
        exact_released: BudgetAmounts| #![auto]
        exact_charged.spec_equal(receipt.spec_charged())
            && exact_released.spec_equal(receipt.spec_released())
            && crate::reachability::observation_accounting_exact(
                before,
                after,
                &exact_release_state,
                budget_id,
                exact_charged,
                exact_released,
            ));
    let index = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(),
                observation.spec_reservation_id(),
            )
            && BudgetAmounts::spec_difference(
                charged,
                observation.spec_cumulative(),
                before.reservations[index].observed,
            )
            && BudgetAmounts::spec_difference(
                released,
                before.reservations[index].request.spec_reserve(),
                observation.spec_cumulative(),
            );
    assert(BudgetAmounts::spec_difference(
        receipt.spec_charged(),
        observation.spec_cumulative(),
        before.reservations[index].observed,
    ));
    assert(BudgetAmounts::spec_difference(
        receipt.spec_released(),
        before.reservations[index].request.spec_reserve(),
        observation.spec_cumulative(),
    ));
    crate::reachability::observation_refines(
        before,
        after,
        observation,
        receipt,
        budget_id,
    );
}

pub(super) proof fn interim_observation_refines(
    before: &BudgetLedger,
    after: &BudgetLedger,
    released_state: &BudgetLedger,
    charged_state: &BudgetLedger,
    observation: UsageObservation,
    receipt: BudgetReceipt,
    budget_id: peritus_types::BudgetId,
    charged: BudgetAmounts,
)
    requires
        crate::reachability::accepted_guard(
            before,
            crate::BudgetCommand::ObserveUsage(observation),
            receipt.spec_kind(),
        ),
        crate::reachability::reservation_bound_to_budget(
            before,
            observation.spec_reservation_id(),
            budget_id,
        ),
        crate::reachability::observation_receipt_exact(receipt, observation, budget_id),
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied,
        observation.spec_finality() == UsageFinality::Interim,
        crate::reachability::operation_release_exact(
            before,
            released_state,
            budget_id,
            charged,
        ),
        crate::reachability::lineage_charge_exact(
            released_state,
            charged_state,
            budget_id,
            charged,
        ),
        crate::identity_model::budget_ids_equal(charged_state.root_id, after.root_id),
        charged_state.accounts@ == after.accounts@,
        charged.spec_equal(receipt.spec_charged()),
        receipt.spec_released().spec_is_zero(),
        exists |index: int| #![auto]
            0 <= index < before.reservations@.len()
                && crate::identity_model::reservation_ids_equal(
                    before.reservations[index].request.spec_reservation_id(),
                    observation.spec_reservation_id(),
                )
                && BudgetAmounts::spec_difference(
                    charged,
                    observation.spec_cumulative(),
                    before.reservations[index].observed,
                ),
        crate::reachability::observation_record_exact(
            before,
            after,
            observation,
            observation.spec_cumulative(),
            ReservationPhase::Active,
            None,
            None,
        ),
    ensures crate::reachability::candidate_step(
        before,
        crate::BudgetCommand::ObserveUsage(observation),
        after,
        receipt,
    ),
{
    assert(crate::reachability::observation_accounting_exact(
        before,
        after,
        released_state,
        budget_id,
        charged,
        receipt.spec_released(),
    )) by {
        assert(exists |exact_charged_state: BudgetLedger| #![auto]
            crate::reachability::lineage_charge_exact(
                released_state,
                &exact_charged_state,
                budget_id,
                charged,
            ) && crate::identity_model::budget_ids_equal(
                exact_charged_state.root_id,
                after.root_id,
            ) && exact_charged_state.accounts@ == after.accounts@);
    }
    assert(exists |exact_release_state: BudgetLedger, exact_charged: BudgetAmounts,
        exact_released: BudgetAmounts| #![auto]
        exact_charged.spec_equal(receipt.spec_charged())
            && exact_released.spec_equal(receipt.spec_released())
            && crate::reachability::observation_accounting_exact(
                before,
                after,
                &exact_release_state,
                budget_id,
                exact_charged,
                exact_released,
            ));
    let index = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(),
                observation.spec_reservation_id(),
            )
            && BudgetAmounts::spec_difference(
                charged,
                observation.spec_cumulative(),
                before.reservations[index].observed,
            );
    assert(BudgetAmounts::spec_difference(
        receipt.spec_charged(),
        observation.spec_cumulative(),
        before.reservations[index].observed,
    ));
    crate::reachability::observation_refines(
        before,
        after,
        observation,
        receipt,
        budget_id,
    );
}

} // verus!
