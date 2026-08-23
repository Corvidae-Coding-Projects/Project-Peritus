//! Functionality of high-water observation candidates.

#[cfg(verus_only)]
use crate::{
    BudgetCommand, BudgetLedger, BudgetReceipt, BudgetReceiptKind, ReservationPhase,
    UsageFinality, UsageObservation,
};
use vstd::prelude::*;

mod applied;
mod overrun_witness;

#[cfg(verus_only)]
use overrun_witness::{
    overrun_charged_witness, overrun_effect_has_release, overrun_release_has_charged,
    overrun_release_witness,
};

verus! {

proof fn equal_preserves_upper_bound(
    equal_left: crate::BudgetAmounts,
    equal_right: crate::BudgetAmounts,
    upper: crate::BudgetAmounts,
)
    requires equal_left.spec_equal(equal_right), equal_left.spec_le(upper),
    ensures equal_right.spec_le(upper),
{
}

proof fn observation_kinds_equal(
    left: &BudgetLedger,
    right: &BudgetLedger,
    observation: UsageObservation,
    left_kind: BudgetReceiptKind,
    right_kind: BudgetReceiptKind,
)
    requires
        crate::model::ledger_well_formed(left),
        left.reservations@ == right.reservations@,
        super::super::guards::observation_guard(left, observation, left_kind),
        super::super::guards::observation_guard(right, observation, right_kind),
    ensures left_kind == right_kind,
{
    reveal(super::super::guards::observation_guard);
    let left_index = choose |index: int| #![auto]
        super::super::guards::reservation_at(
            left, observation.spec_reservation_id(), index,
        ) && match left.reservations[index].phase {
            ReservationPhase::Active => match left_kind {
                BudgetReceiptKind::Idempotent => {
                    left.reservations[index].observed.spec_equal(
                        observation.spec_cumulative(),
                    ) && crate::invariant::optional_digests_equal(
                        left.reservations[index].observation_evidence,
                        Some(observation.spec_evidence_digest()),
                    ) && observation.spec_finality() == UsageFinality::Interim
                }
                BudgetReceiptKind::Applied => {
                    left.reservations[index].observed.spec_le(
                        observation.spec_cumulative(),
                    ) && observation.spec_cumulative().spec_le(
                        left.reservations[index].request.spec_reserve(),
                    ) && !(left.reservations[index].observed.spec_equal(
                        observation.spec_cumulative(),
                    ) && crate::invariant::optional_digests_equal(
                        left.reservations[index].observation_evidence,
                        Some(observation.spec_evidence_digest()),
                    ) && observation.spec_finality() == UsageFinality::Interim)
                }
                BudgetReceiptKind::OverrunFaulted => {
                    left.reservations[index].observed.spec_le(
                        observation.spec_cumulative(),
                    ) && !observation.spec_cumulative().spec_le(
                        left.reservations[index].request.spec_reserve(),
                    )
                }
            },
            ReservationPhase::SettledFinal => left_kind == BudgetReceiptKind::Idempotent,
            ReservationPhase::OverrunFaulted => {
                left_kind == BudgetReceiptKind::OverrunFaulted
            }
            _ => false,
        };
    let right_index = choose |index: int| #![auto]
        super::super::guards::reservation_at(
            right, observation.spec_reservation_id(), index,
        ) && match right.reservations[index].phase {
            ReservationPhase::Active => match right_kind {
                BudgetReceiptKind::Idempotent => {
                    right.reservations[index].observed.spec_equal(
                        observation.spec_cumulative(),
                    ) && crate::invariant::optional_digests_equal(
                        right.reservations[index].observation_evidence,
                        Some(observation.spec_evidence_digest()),
                    ) && observation.spec_finality() == UsageFinality::Interim
                }
                BudgetReceiptKind::Applied => {
                    right.reservations[index].observed.spec_le(
                        observation.spec_cumulative(),
                    ) && observation.spec_cumulative().spec_le(
                        right.reservations[index].request.spec_reserve(),
                    ) && (!right.reservations[index].observed.spec_equal(
                        observation.spec_cumulative(),
                    ) || right.reservations[index].observation_evidence.is_none()
                        || crate::invariant::optional_digests_equal(
                            right.reservations[index].observation_evidence,
                            Some(observation.spec_evidence_digest()),
                        )) && !(right.reservations[index].observed.spec_equal(
                        observation.spec_cumulative(),
                    ) && crate::invariant::optional_digests_equal(
                        right.reservations[index].observation_evidence,
                        Some(observation.spec_evidence_digest()),
                    ) && observation.spec_finality() == UsageFinality::Interim)
                }
                BudgetReceiptKind::OverrunFaulted => {
                    right.reservations[index].observed.spec_le(
                        observation.spec_cumulative(),
                    ) && !observation.spec_cumulative().spec_le(
                        right.reservations[index].request.spec_reserve(),
                    )
                }
            },
            ReservationPhase::SettledFinal => right_kind == BudgetReceiptKind::Idempotent,
            ReservationPhase::OverrunFaulted => {
                right_kind == BudgetReceiptKind::OverrunFaulted
            }
            _ => false,
        };
    crate::invariant::matching_reservations_are_unique(left, left_index, right_index);
    assert(left_index == right_index);
    assert(crate::invariant::reservation_entry_valid(left, left_index));
    match left.reservations[left_index].phase {
        ReservationPhase::Active => {
            match (left_kind, right_kind) {
                (BudgetReceiptKind::Idempotent, BudgetReceiptKind::Idempotent)
                | (BudgetReceiptKind::Applied, BudgetReceiptKind::Applied)
                | (BudgetReceiptKind::OverrunFaulted, BudgetReceiptKind::OverrunFaulted) => {}
                (BudgetReceiptKind::Idempotent, BudgetReceiptKind::OverrunFaulted) => {
                    equal_preserves_upper_bound(
                        left.reservations[left_index].observed,
                        observation.spec_cumulative(),
                        left.reservations[left_index].request.spec_reserve(),
                    );
                    assert(false);
                }
                (BudgetReceiptKind::OverrunFaulted, BudgetReceiptKind::Idempotent) => {
                    equal_preserves_upper_bound(
                        left.reservations[left_index].observed,
                        observation.spec_cumulative(),
                        left.reservations[left_index].request.spec_reserve(),
                    );
                    assert(false);
                }
                _ => assert(false),
            }
        }
        ReservationPhase::SettledFinal | ReservationPhase::OverrunFaulted => {}
        _ => assert(false),
    }
}

pub(super) proof fn observation_candidates_equal(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    observation: UsageObservation,
    left_after: &BudgetLedger,
    left_receipt: BudgetReceipt,
    right_after: &BudgetLedger,
    right_receipt: BudgetReceipt,
)
    requires
        left_before.accounts@ == right_before.accounts@,
        left_before.reservations@ == right_before.reservations@,
        super::super::raw_accepted_step(
            left_before, BudgetCommand::ObserveUsage(observation),
            left_after, left_receipt,
        ),
        super::super::raw_accepted_step(
            right_before, BudgetCommand::ObserveUsage(observation),
            right_after, right_receipt,
        ),
    ensures
        super::super::commands::ledger_views_equal(left_after, right_after),
        super::super::commands::receipts_exactly_equal(left_receipt, right_receipt),
{
    reveal(super::super::raw_accepted_step);
    reveal(super::super::guards::accepted_command_guard);
    reveal(super::super::commands::observation_step);
    reveal(super::super::commands::observation_budget_step);
    assert(crate::model::ledger_well_formed(left_before));
    observation_kinds_equal(
        left_before, right_before, observation,
        left_receipt.spec_kind(), right_receipt.spec_kind(),
    );
    let left_index = choose |index: int| #![auto]
        super::super::guards::reservation_at(
            left_before, observation.spec_reservation_id(), index,
        );
    let right_index = choose |index: int| #![auto]
        super::super::guards::reservation_at(
            right_before, observation.spec_reservation_id(), index,
        );
    crate::invariant::matching_reservations_are_unique(
        left_before, left_index, right_index,
    );
    assert(left_index == right_index);
    let left_budget = choose |budget: peritus_types::BudgetId| #![auto]
        super::super::commands::observation_budget_step(
            left_before, observation, left_after, left_receipt, budget,
        );
    let right_budget = choose |budget: peritus_types::BudgetId| #![auto]
        super::super::commands::observation_budget_step(
            right_before, observation, right_after, right_receipt, budget,
        );
    super::finalization::bound_budgets_equal(
        left_before, right_before, observation.spec_reservation_id(),
        left_budget, right_budget,
    );
    super::accounting::well_formed_has_unique_account_ids(left_before);
    match left_receipt.spec_kind() {
        BudgetReceiptKind::Idempotent => {}
        BudgetReceiptKind::Applied => {
            applied::applied_candidates_equal(
                left_before,
                right_before,
                observation,
                left_after,
                left_receipt,
                right_after,
                right_receipt,
                left_index,
                right_index,
                left_budget,
                right_budget,
            );
        }
        BudgetReceiptKind::OverrunFaulted => {
            reveal(super::super::commands::observation_overrun_effect);
            assert(exists |index: int| #![auto]
                super::super::commands::observation_overrun_effect(
                    left_before, observation, left_after, left_receipt,
                    left_budget, index,
                ));
            assert(exists |index: int| #![auto]
                super::super::commands::observation_overrun_effect(
                    right_before, observation, right_after, right_receipt,
                    right_budget, index,
                ));
            let left_effect_index = choose |index: int| #![auto]
                super::super::commands::observation_overrun_effect(
                    left_before, observation, left_after, left_receipt,
                    left_budget, index,
                );
            let right_effect_index = choose |index: int| #![auto]
                super::super::commands::observation_overrun_effect(
                    right_before, observation, right_after, right_receipt,
                    right_budget, index,
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
                    super::finalization::differences_are_unique(
                        left_receipt.spec_charged(), right_receipt.spec_charged(),
                        left_before.reservations[left_index].request.spec_reserve(),
                        left_before.reservations[left_index].observed,
                    );
                    reveal(overrun_release_witness);
                    reveal(overrun_charged_witness);
                    overrun_effect_has_release(
                        left_before, observation, left_after, left_receipt,
                        left_budget, left_effect_index,
                    );
                    let left_released = choose |released: BudgetLedger| #![auto]
                        overrun_release_witness(
                            left_before, left_after, left_budget,
                            left_receipt, released,
                        );
                    overrun_release_has_charged(
                        left_before, left_after, left_budget, left_receipt,
                        &left_released,
                    );
                    let left_charged_state = choose |state: BudgetLedger| #![auto]
                        overrun_charged_witness(
                            left_before, left_after, left_budget,
                            left_receipt, &left_released, state,
                        );
                    let left_exact = choose |exact: crate::BudgetAmounts| #![auto]
                        exact.spec_equal(left_receipt.spec_charged())
                            && super::super::account_updates::overrun_accounting(
                                left_before, left_after, &left_released,
                                &left_charged_state, left_budget, exact,
                            );
                    overrun_effect_has_release(
                        right_before, observation, right_after, right_receipt,
                        right_budget, right_effect_index,
                    );
                    let right_released = choose |released: BudgetLedger| #![auto]
                        overrun_release_witness(
                            right_before, right_after, right_budget,
                            right_receipt, released,
                        );
                    overrun_release_has_charged(
                        right_before, right_after, right_budget, right_receipt,
                        &right_released,
                    );
                    let right_charged_state = choose |state: BudgetLedger| #![auto]
                        overrun_charged_witness(
                            right_before, right_after, right_budget,
                            right_receipt, &right_released, state,
                        );
                    let right_exact = choose |exact: crate::BudgetAmounts| #![auto]
                        exact.spec_equal(right_receipt.spec_charged())
                            && super::super::account_updates::overrun_accounting(
                                right_before, right_after, &right_released,
                                &right_charged_state, right_budget, exact,
                            );
                    super::finalization::amounts_equal_through(
                        left_exact, left_receipt.spec_charged(), right_exact,
                    );
                    super::fault::overrun_functional(
                        left_before, right_before, left_after, right_after,
                        &left_released, &right_released,
                        &left_charged_state, &right_charged_state,
                        left_budget, right_budget, left_exact, right_exact,
                    );
                    assert(super::super::reservations::observation_effect(
                        left_before, right_after, observation.spec_reservation_id(),
                        left_before.reservations[left_index].request.spec_reserve(),
                        observation.spec_evidence_digest(),
                        ReservationPhase::OverrunFaulted,
                        Some(observation.spec_cumulative()),
                        Some(observation.spec_finality()),
                    ));
                    super::reservation_updates::observation_effects_equal(
                        left_before, left_after, right_after,
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
        }
    }
    assert(super::super::commands::ledger_views_equal(left_after, right_after));
    assert(super::super::commands::receipts_exactly_equal(
        left_receipt, right_receipt,
    ));
}

} // verus!
