//! Exact refinement facts for usage observations.

#[cfg(verus_only)]
use crate::{BudgetCommand, BudgetLedger, BudgetReceipt};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn observation_receipt_exact(
    receipt: BudgetReceipt,
    observation: crate::UsageObservation,
    budget_id: peritus_types::BudgetId,
) -> bool {
    crate::reachability::commands::receipt_identity(
        receipt,
        crate::BudgetOperation::ObserveUsage,
        budget_id,
        Some(observation.spec_reservation_id()),
    )
        && crate::invariant::optional_amounts_equal(
            receipt.spec_reported(),
            Some(observation.spec_cumulative()),
        )
        && crate::invariant::optional_digests_equal(
            receipt.spec_evidence_digest(),
            Some(observation.spec_evidence_digest()),
        )
}

pub(crate) open spec fn observation_record_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    observation: crate::UsageObservation,
    observed: crate::BudgetAmounts,
    phase: crate::ReservationPhase,
    final_reported: Option<crate::BudgetAmounts>,
    finality: Option<crate::UsageFinality>,
) -> bool {
    crate::reachability::reservations::observation_effect(
        before,
        after,
        observation.spec_reservation_id(),
        observed,
        observation.spec_evidence_digest(),
        phase,
        final_reported,
        finality,
    )
}

pub(crate) proof fn observation_refines(
    before: &BudgetLedger,
    after: &BudgetLedger,
    observation: crate::UsageObservation,
    receipt: BudgetReceipt,
    budget_id: peritus_types::BudgetId,
)
    requires
        crate::reachability::guards::accepted_command_guard(
            before,
            BudgetCommand::ObserveUsage(observation),
            receipt.spec_kind(),
        ),
        reservation_bound_to_budget(before, observation.spec_reservation_id(), budget_id),
        observation_receipt_exact(receipt, observation, budget_id),
        match receipt.spec_kind() {
            crate::BudgetReceiptKind::Idempotent => {
                receipt.spec_charged().spec_is_zero()
                    && receipt.spec_released().spec_is_zero()
                    && crate::reachability::ledgers_exactly_equal(before, after)
            }
            crate::BudgetReceiptKind::Applied => {
                (exists |index: int| #![auto]
                    0 <= index < before.reservations@.len()
                        && crate::identity_model::reservation_ids_equal(
                            before.reservations[index].request.spec_reservation_id(),
                            observation.spec_reservation_id(),
                        )
                        && crate::BudgetAmounts::spec_difference(
                            receipt.spec_charged(),
                            observation.spec_cumulative(),
                            before.reservations[index].observed,
                        )
                        && if observation.spec_finality() == crate::UsageFinality::Final {
                            crate::BudgetAmounts::spec_difference(
                                receipt.spec_released(),
                                before.reservations[index].request.spec_reserve(),
                                observation.spec_cumulative(),
                            )
                        } else {
                            receipt.spec_released().spec_is_zero()
                        })
                    && (exists |released_state: BudgetLedger,
                        exact_charged: crate::BudgetAmounts,
                        exact_released: crate::BudgetAmounts| #![auto]
                        exact_charged.spec_equal(receipt.spec_charged())
                            && exact_released.spec_equal(receipt.spec_released())
                            && crate::reachability::account_updates::reservation_accounting(
                            before,
                            after,
                            &released_state,
                            budget_id,
                            exact_charged,
                            exact_released,
                        ))
                    && observation_record_exact(
                        before,
                        after,
                        observation,
                        observation.spec_cumulative(),
                        if observation.spec_finality() == crate::UsageFinality::Final {
                            crate::ReservationPhase::SettledFinal
                        } else {
                            crate::ReservationPhase::Active
                        },
                        if observation.spec_finality() == crate::UsageFinality::Final {
                            Some(observation.spec_cumulative())
                        } else {
                            None
                        },
                        if observation.spec_finality() == crate::UsageFinality::Final {
                            Some(crate::UsageFinality::Final)
                        } else {
                            None
                        },
                    )
            }
            crate::BudgetReceiptKind::OverrunFaulted => {
                exists |index: int| #![auto]
                    crate::reachability::commands::observation_overrun_effect(
                        before, observation, after, receipt, budget_id, index,
                    )
            }
        },
    ensures
        crate::reachability::candidate_step(
            before,
            BudgetCommand::ObserveUsage(observation),
            after,
            receipt,
        ),
{
    reveal(crate::reachability::commands::observation_step);
    reveal(crate::reachability::commands::observation_budget_step);
    reveal(crate::reachability::commands::observation_applied_delta);
    reveal(crate::reachability::commands::observation_overrun_effect);
    reveal(observation_receipt_exact);
    reveal(observation_record_exact);
    reveal(reservation_bound_to_budget);
    match receipt.spec_kind() {
        crate::BudgetReceiptKind::Applied => {
            crate::reachability::commands::observation_applied_delta_from_fields(
                before, observation, receipt,
            );
        }
        crate::BudgetReceiptKind::OverrunFaulted => {
            assert(exists |index: int| #![auto]
                crate::reachability::commands::observation_overrun_effect(
                    before, observation, after, receipt, budget_id, index,
                ));
        }
        crate::BudgetReceiptKind::Idempotent => {}
    }
    assert(crate::reachability::commands::observation_budget_step(
        before, observation, after, receipt, budget_id,
    ));
    crate::reachability::raw_step_is_accepted(
        before,
        BudgetCommand::ObserveUsage(observation),
        after,
        receipt,
    );
}

pub(crate) proof fn overrun_active_effect_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    observation: crate::UsageObservation,
    receipt: BudgetReceipt,
    budget_id: peritus_types::BudgetId,
    index: int,
    released_state: &BudgetLedger,
    charged_state: &BudgetLedger,
    exact_charged: crate::BudgetAmounts,
)
    requires
        0 <= index < before.reservations@.len(),
        crate::identity_model::reservation_ids_equal(
            before.reservations[index].request.spec_reservation_id(),
            observation.spec_reservation_id(),
        ),
        before.reservations[index].phase == crate::ReservationPhase::Active,
        receipt.spec_released().spec_is_zero(),
        crate::BudgetAmounts::spec_difference(
            receipt.spec_charged(),
            before.reservations[index].request.spec_reserve(),
            before.reservations[index].observed,
        ),
        exact_charged.spec_equal(receipt.spec_charged()),
        crate::reachability::account_updates::overrun_accounting(
            before, after, released_state, charged_state, budget_id, exact_charged,
        ),
        observation_record_exact(
            before, after, observation,
            before.reservations[index].request.spec_reserve(),
            crate::ReservationPhase::OverrunFaulted,
            Some(observation.spec_cumulative()),
            Some(observation.spec_finality()),
        ),
    ensures crate::reachability::commands::observation_overrun_effect(
        before, observation, after, receipt, budget_id, index,
    ),
{
    reveal(crate::reachability::commands::observation_overrun_effect);
    reveal(crate::reachability::commands::observation_overrun_release_effect);
    reveal(crate::reachability::commands::observation_overrun_charged_effect);
    reveal(observation_record_exact);
    assert(crate::reachability::commands::observation_overrun_charged_effect(
        before, after, receipt, budget_id, released_state, *charged_state,
    ));
    assert(crate::reachability::commands::observation_overrun_release_effect(
        before, after, receipt, budget_id, *released_state,
    ));
}

pub(crate) proof fn overrun_terminal_effect_exact(
    before: &BudgetLedger,
    observation: crate::UsageObservation,
    receipt: BudgetReceipt,
    budget_id: peritus_types::BudgetId,
    index: int,
)
    requires
        0 <= index < before.reservations@.len(),
        crate::identity_model::reservation_ids_equal(
            before.reservations[index].request.spec_reservation_id(),
            observation.spec_reservation_id(),
        ),
        before.reservations[index].phase == crate::ReservationPhase::SettledFinal
            || before.reservations[index].phase == crate::ReservationPhase::OverrunFaulted,
        crate::reachability::ledgers_exactly_equal(before, before),
        receipt.spec_charged().spec_is_zero(),
        receipt.spec_released().spec_is_zero(),
    ensures crate::reachability::commands::observation_overrun_effect(
        before, observation, before, receipt, budget_id, index,
    ),
{
    reveal(crate::reachability::commands::observation_overrun_effect);
}

pub(crate) open spec fn reservation_bound_to_budget(
    ledger: &BudgetLedger,
    reservation_id: peritus_types::BudgetReservationId,
    budget_id: peritus_types::BudgetId,
) -> bool {
    crate::reachability::commands::bound_budget(ledger, reservation_id, budget_id)
}

} // verus!
