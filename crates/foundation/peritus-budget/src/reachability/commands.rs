//! Closed exact accepted-step relations for every budget command.

#[cfg(verus_only)]
use crate::{
    BudgetAmounts, BudgetLedger, BudgetOperation, BudgetReceipt, BudgetReceiptKind,
    ReservationPhase, UsageFinality,
};
#[cfg(verus_only)]
use peritus_types::{BudgetId, BudgetReservationId};
use vstd::prelude::*;

mod equality;

#[cfg(verus_only)]
pub(crate) use equality::{
    accounts_exactly_equal, bound_budget, ledger_equality_reflexive, ledger_views_equal,
    ledgers_exactly_equal, receipt_has_no_observation, receipt_identity,
    receipts_exactly_equal, reservations_exactly_equal,
};

verus! {

pub(crate) open spec fn allocate_child_step(
    before: &BudgetLedger,
    request: crate::ChildBudgetRequest,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
) -> bool {
    receipt_identity(
        receipt,
        BudgetOperation::AllocateChild,
        request.spec_child_id(),
        None,
    )
        && receipt.spec_charged().spec_is_zero()
        && receipt.spec_released().spec_is_zero()
        && receipt_has_no_observation(receipt)
        && match receipt.spec_kind() {
            BudgetReceiptKind::Idempotent => ledgers_exactly_equal(before, after),
            BudgetReceiptKind::Applied => {
                super::allocation::child_allocation_effect(before, after, request)
            }
            BudgetReceiptKind::OverrunFaulted => false,
        }
}

pub(crate) open spec fn begin_step(
    before: &BudgetLedger,
    request: crate::BudgetRequest,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
) -> bool {
    receipt_identity(
        receipt,
        BudgetOperation::Begin,
        request.spec_budget_id(),
        Some(request.spec_reservation_id()),
    )
        && receipt.spec_released().spec_is_zero()
        && receipt_has_no_observation(receipt)
        && match receipt.spec_kind() {
            BudgetReceiptKind::Idempotent => {
                receipt.spec_charged().spec_is_zero() && ledgers_exactly_equal(before, after)
            }
            BudgetReceiptKind::Applied => {
                receipt.spec_charged().spec_equal(request.spec_consume_now())
                    && (exists |charged_state: BudgetLedger| #![auto]
                        super::account_updates::begin_accounting(
                            before,
                            after,
                            &charged_state,
                            request.spec_budget_id(),
                            request.spec_consume_now(),
                            request.spec_reserve(),
                        ))
                    && super::reservations::begin_record_effect(before, after, request)
            }
            BudgetReceiptKind::OverrunFaulted => false,
        }
}

pub(crate) open spec fn activate_step(
    before: &BudgetLedger,
    activation: crate::Activation,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
) -> bool {
    exists |budget_id: BudgetId| #![auto]
        bound_budget(before, activation.spec_reservation_id(), budget_id)
            && receipt_identity(
                receipt,
                BudgetOperation::Activate,
                budget_id,
                Some(activation.spec_reservation_id()),
            )
            && receipt.spec_charged().spec_is_zero()
            && receipt.spec_released().spec_is_zero()
            && receipt.spec_reported().is_none()
            && crate::invariant::optional_digests_equal(
                receipt.spec_evidence_digest(),
                Some(activation.spec_evidence_digest()),
            )
            && accounts_exactly_equal(before, after)
            && match receipt.spec_kind() {
                BudgetReceiptKind::Idempotent => ledgers_exactly_equal(before, after),
                BudgetReceiptKind::Applied => super::reservations::activation_effect(
                    before,
                    after,
                    activation.spec_reservation_id(),
                    activation.spec_evidence_digest(),
                ),
                BudgetReceiptKind::OverrunFaulted => false,
            }
}

pub(crate) open spec fn observation_overrun_charged_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
    budget_id: BudgetId,
    released_state: &BudgetLedger,
    charged_state: BudgetLedger,
) -> bool {
    exists |exact_charged: BudgetAmounts| #![auto]
        exact_charged.spec_equal(receipt.spec_charged())
            && super::account_updates::overrun_accounting(
                before, after, released_state, &charged_state,
                budget_id, exact_charged,
            )
}

pub(crate) open spec fn observation_overrun_release_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
    budget_id: BudgetId,
    released_state: BudgetLedger,
) -> bool {
    exists |charged_state: BudgetLedger| #![auto]
        observation_overrun_charged_effect(
            before, after, receipt, budget_id, &released_state, charged_state,
        )
}

pub(crate) open spec fn observation_overrun_effect(
    before: &BudgetLedger,
    observation: crate::UsageObservation,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
    budget_id: BudgetId,
    index: int,
) -> bool {
    0 <= index < before.reservations@.len()
        && crate::identity_model::reservation_ids_equal(
            before.reservations[index].request.spec_reservation_id(),
            observation.spec_reservation_id(),
        )
        && match before.reservations[index].phase {
            ReservationPhase::SettledFinal | ReservationPhase::OverrunFaulted => {
                ledgers_exactly_equal(before, after)
                    && receipt.spec_charged().spec_is_zero()
                    && receipt.spec_released().spec_is_zero()
            }
            ReservationPhase::Active => {
                receipt.spec_released().spec_is_zero()
                    && BudgetAmounts::spec_difference(
                        receipt.spec_charged(),
                        before.reservations[index].request.spec_reserve(),
                        before.reservations[index].observed,
                    )
                    && (exists |released_state: BudgetLedger| #![auto]
                        observation_overrun_release_effect(
                            before, after, receipt, budget_id, released_state,
                        ))
                    && super::reservations::observation_effect(
                        before,
                        after,
                        observation.spec_reservation_id(),
                        before.reservations[index].request.spec_reserve(),
                        observation.spec_evidence_digest(),
                        ReservationPhase::OverrunFaulted,
                        Some(observation.spec_cumulative()),
                        Some(observation.spec_finality()),
                    )
            }
            _ => false,
        }
}

pub(crate) open spec fn observation_applied_delta(
    before: &BudgetLedger,
    observation: crate::UsageObservation,
    receipt: BudgetReceipt,
    index: int,
) -> bool {
    0 <= index < before.reservations@.len()
        && crate::identity_model::reservation_ids_equal(
            before.reservations[index].request.spec_reservation_id(),
            observation.spec_reservation_id(),
        )
        && BudgetAmounts::spec_difference(
            receipt.spec_charged(),
            observation.spec_cumulative(),
            before.reservations[index].observed,
        )
        && if observation.spec_finality() == UsageFinality::Final {
            BudgetAmounts::spec_difference(
                receipt.spec_released(),
                before.reservations[index].request.spec_reserve(),
                observation.spec_cumulative(),
            )
        } else {
            receipt.spec_released().spec_is_zero()
        }
}

pub(crate) proof fn observation_applied_delta_from_fields(
    before: &BudgetLedger,
    observation: crate::UsageObservation,
    receipt: BudgetReceipt,
)
    requires exists |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(),
                observation.spec_reservation_id(),
            )
            && BudgetAmounts::spec_difference(
                receipt.spec_charged(), observation.spec_cumulative(),
                before.reservations[index].observed,
            )
            && if observation.spec_finality() == UsageFinality::Final {
                BudgetAmounts::spec_difference(
                    receipt.spec_released(),
                    before.reservations[index].request.spec_reserve(),
                    observation.spec_cumulative(),
                )
            } else {
                receipt.spec_released().spec_is_zero()
            },
    ensures exists |index: int| #![auto]
        observation_applied_delta(before, observation, receipt, index),
{
    reveal(observation_applied_delta);
    let index = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(),
                observation.spec_reservation_id(),
            )
            && BudgetAmounts::spec_difference(
                receipt.spec_charged(), observation.spec_cumulative(),
                before.reservations[index].observed,
            )
            && if observation.spec_finality() == UsageFinality::Final {
                BudgetAmounts::spec_difference(
                    receipt.spec_released(),
                    before.reservations[index].request.spec_reserve(),
                    observation.spec_cumulative(),
                )
            } else {
                receipt.spec_released().spec_is_zero()
            };
    assert(observation_applied_delta(before, observation, receipt, index));
    assert(exists |witness: int| #![auto]
        observation_applied_delta(before, observation, receipt, witness));
}

pub(crate) open spec fn observation_step(
    before: &BudgetLedger,
    observation: crate::UsageObservation,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
) -> bool {
    exists |budget_id: BudgetId| #![auto]
        observation_budget_step(before, observation, after, receipt, budget_id)
}

pub(crate) open spec fn observation_budget_step(
    before: &BudgetLedger,
    observation: crate::UsageObservation,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
    budget_id: BudgetId,
) -> bool {
    bound_budget(before, observation.spec_reservation_id(), budget_id)
            && receipt_identity(
                receipt,
                BudgetOperation::ObserveUsage,
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
            && match receipt.spec_kind() {
                BudgetReceiptKind::Idempotent => {
                    receipt.spec_charged().spec_is_zero()
                        && receipt.spec_released().spec_is_zero()
                        && ledgers_exactly_equal(before, after)
                }
                BudgetReceiptKind::Applied => {
                    (exists |index: int| #![auto]
                        observation_applied_delta(before, observation, receipt, index))
                        && (exists |released_state: BudgetLedger, exact_charged: BudgetAmounts,
                            exact_released: BudgetAmounts| #![auto]
                            exact_charged.spec_equal(receipt.spec_charged())
                                && exact_released.spec_equal(receipt.spec_released())
                                && super::account_updates::reservation_accounting(
                                before,
                                after,
                                &released_state,
                                budget_id,
                                exact_charged,
                                exact_released,
                            ))
                        && super::reservations::observation_effect(
                            before,
                            after,
                            observation.spec_reservation_id(),
                            observation.spec_cumulative(),
                            observation.spec_evidence_digest(),
                            if observation.spec_finality() == UsageFinality::Final {
                                ReservationPhase::SettledFinal
                            } else {
                                ReservationPhase::Active
                            },
                            if observation.spec_finality() == UsageFinality::Final {
                                Some(observation.spec_cumulative())
                            } else {
                                None
                            },
                            if observation.spec_finality() == UsageFinality::Final {
                                Some(UsageFinality::Final)
                            } else {
                                None
                            },
                        )
                }
                BudgetReceiptKind::OverrunFaulted => {
                    exists |index: int| #![auto]
                        observation_overrun_effect(
                            before, observation, after, receipt, budget_id, index,
                        )
                }
            }
}

} // verus!
