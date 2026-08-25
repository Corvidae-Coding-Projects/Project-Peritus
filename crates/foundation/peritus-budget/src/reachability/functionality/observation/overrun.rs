//! Exact functionality of accepted over-ceiling observation candidates.

#[cfg(verus_only)]
use crate::{BudgetLedger, BudgetReceipt, BudgetReceiptKind, ReservationPhase, UsageObservation};
#[cfg(verus_only)]
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

proof fn active_overrun_has_accounting_witness(
    before: &BudgetLedger,
    observation: UsageObservation,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
    budget: BudgetId,
    index: int,
)
    requires
        crate::reachability::commands::observation_overrun_effect(
            before, observation, after, receipt, budget, index,
        ),
        before.reservations[index].phase == ReservationPhase::Active,
    ensures
        exists |witness: (BudgetLedger, BudgetLedger, crate::BudgetAmounts)| #![auto]
            witness.2.spec_equal(receipt.spec_charged())
                && crate::reachability::account_updates::overrun_accounting(
                    before,
                    after,
                    &witness.0,
                    &witness.1,
                    budget,
                    witness.2,
                ),
{
    super::overrun_witness::overrun_effect_has_release(
        before, observation, after, receipt, budget, index,
    );
    let released = choose |released: BudgetLedger| #![auto]
        super::overrun_witness::overrun_release_witness(
            before, after, budget, receipt, released,
        );
    super::overrun_witness::overrun_release_has_charged(
        before, after, budget, receipt, &released,
    );
    let charged = choose |state: BudgetLedger| #![auto]
        super::overrun_witness::overrun_charged_witness(
            before, after, budget, receipt, &released, state,
        );
    reveal(super::overrun_witness::overrun_charged_witness);
    let exact = choose |exact: crate::BudgetAmounts| #![auto]
        exact.spec_equal(receipt.spec_charged())
            && crate::reachability::account_updates::overrun_accounting(
                before, after, &released, &charged, budget, exact,
            );
    let witness = (released, charged, exact);
    assert(exists |candidate: (BudgetLedger, BudgetLedger, crate::BudgetAmounts)| #![auto]
        candidate.2.spec_equal(receipt.spec_charged())
            && crate::reachability::account_updates::overrun_accounting(
                before,
                after,
                &candidate.0,
                &candidate.1,
                budget,
                candidate.2,
            )) by {
        assert(witness.2.spec_equal(receipt.spec_charged()));
        assert(crate::reachability::account_updates::overrun_accounting(
            before,
            after,
            &witness.0,
            &witness.1,
            budget,
            witness.2,
        ));
    }
}

#[verifier::spinoff_prover]
#[allow(
    clippy::too_many_arguments,
    reason = "The account refinement compares both exact overrun witness chains"
)]
proof fn active_overrun_accounts_equal(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    observation: UsageObservation,
    left_after: &BudgetLedger,
    left_receipt: BudgetReceipt,
    right_after: &BudgetLedger,
    right_receipt: BudgetReceipt,
    index: int,
    left_budget: BudgetId,
    right_budget: BudgetId,
    left_effect_index: int,
    right_effect_index: int,
)
    requires
        crate::model::ledger_well_formed(left_before),
        crate::model::ledger_well_formed(right_before),
        left_before.accounts@ == right_before.accounts@,
        left_before.reservations@ == right_before.reservations@,
        crate::identity_model::budget_ids_equal(left_budget, right_budget),
        left_receipt.spec_charged().spec_equal(right_receipt.spec_charged()),
        crate::reachability::commands::observation_overrun_effect(
            left_before,
            observation,
            left_after,
            left_receipt,
            left_budget,
            left_effect_index,
        ),
        crate::reachability::commands::observation_overrun_effect(
            right_before,
            observation,
            right_after,
            right_receipt,
            right_budget,
            right_effect_index,
        ),
        left_before.reservations[index].phase == ReservationPhase::Active,
        left_effect_index == index,
        right_effect_index == index,
    ensures
        crate::reachability::functionality::accounting::account_sequences_equal(
            left_after.accounts@,
            right_after.accounts@,
        ),
{
    active_overrun_has_accounting_witness(
        left_before,
        observation,
        left_after,
        left_receipt,
        left_budget,
        left_effect_index,
    );
    let left_witness = choose |witness: (
        BudgetLedger,
        BudgetLedger,
        crate::BudgetAmounts,
    )| #![auto]
        witness.2.spec_equal(left_receipt.spec_charged())
            && crate::reachability::account_updates::overrun_accounting(
                left_before,
                left_after,
                &witness.0,
                &witness.1,
                left_budget,
                witness.2,
            );
    active_overrun_has_accounting_witness(
        right_before,
        observation,
        right_after,
        right_receipt,
        right_budget,
        right_effect_index,
    );
    let right_witness = choose |witness: (
        BudgetLedger,
        BudgetLedger,
        crate::BudgetAmounts,
    )| #![auto]
        witness.2.spec_equal(right_receipt.spec_charged())
            && crate::reachability::account_updates::overrun_accounting(
                right_before,
                right_after,
                &witness.0,
                &witness.1,
                right_budget,
                witness.2,
            );
    super::super::finalization::amounts_equal_through(
        left_witness.2,
        left_receipt.spec_charged(),
        right_witness.2,
    );
    super::super::accounting::well_formed_has_unique_account_ids(left_before);
    super::super::fault::overrun_functional(
        left_before,
        right_before,
        left_after,
        right_after,
        &left_witness.0,
        &right_witness.0,
        &left_witness.1,
        &right_witness.1,
        left_budget,
        right_budget,
        left_witness.2,
        right_witness.2,
    );
}

#[verifier::spinoff_prover]
#[allow(
    clippy::too_many_arguments,
    reason = "The active-overrun refinement compares both exact candidate witnesses"
)]
proof fn active_overrun_candidates_equal(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    observation: UsageObservation,
    left_after: &BudgetLedger,
    left_receipt: BudgetReceipt,
    right_after: &BudgetLedger,
    right_receipt: BudgetReceipt,
    index: int,
    left_budget: BudgetId,
    right_budget: BudgetId,
    left_effect_index: int,
    right_effect_index: int,
)
    requires
        crate::model::ledger_well_formed(left_before),
        crate::model::ledger_well_formed(right_before),
        left_before.accounts@ == right_before.accounts@,
        left_before.reservations@ == right_before.reservations@,
        crate::reachability::guards::reservation_at(
            left_before, observation.spec_reservation_id(), index,
        ),
        left_before.reservations[index].phase == ReservationPhase::Active,
        crate::identity_model::budget_ids_equal(left_budget, right_budget),
        left_receipt.spec_kind() == BudgetReceiptKind::OverrunFaulted,
        right_receipt.spec_kind() == BudgetReceiptKind::OverrunFaulted,
        crate::reachability::commands::observation_budget_step(
            left_before, observation, left_after, left_receipt, left_budget,
        ),
        crate::reachability::commands::observation_budget_step(
            right_before, observation, right_after, right_receipt, right_budget,
        ),
        crate::reachability::commands::observation_overrun_effect(
            left_before,
            observation,
            left_after,
            left_receipt,
            left_budget,
            left_effect_index,
        ),
        crate::reachability::commands::observation_overrun_effect(
            right_before,
            observation,
            right_after,
            right_receipt,
            right_budget,
            right_effect_index,
        ),
        left_effect_index == index,
        right_effect_index == index,
    ensures
        crate::reachability::commands::ledger_views_equal(left_after, right_after),
        crate::reachability::commands::receipts_exactly_equal(left_receipt, right_receipt),
{
    reveal(crate::reachability::commands::observation_budget_step);
    reveal(crate::reachability::commands::observation_overrun_effect);
    super::super::finalization::differences_are_unique(
        left_receipt.spec_charged(),
        right_receipt.spec_charged(),
        left_before.reservations[index].request.spec_reserve(),
        left_before.reservations[index].observed,
    );
    active_overrun_accounts_equal(
        left_before,
        right_before,
        observation,
        left_after,
        left_receipt,
        right_after,
        right_receipt,
        index,
        left_budget,
        right_budget,
        left_effect_index,
        right_effect_index,
    );
    assert(crate::reachability::reservations::observation_effect(
        left_before,
        right_after,
        observation.spec_reservation_id(),
        left_before.reservations[index].request.spec_reserve(),
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
        left_before.reservations[index].request.spec_reserve(),
        observation.spec_evidence_digest(),
        ReservationPhase::OverrunFaulted,
        Some(observation.spec_cumulative()),
        Some(observation.spec_finality()),
    );
    assert(crate::reachability::commands::ledger_views_equal(left_after, right_after));
    assert(crate::reachability::commands::receipts_exactly_equal(
        left_receipt, right_receipt,
    ));
}

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
            active_overrun_candidates_equal(
                left_before,
                right_before,
                observation,
                left_after,
                left_receipt,
                right_after,
                right_receipt,
                left_index,
                left_budget,
                right_budget,
                left_effect_index,
                right_effect_index,
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
