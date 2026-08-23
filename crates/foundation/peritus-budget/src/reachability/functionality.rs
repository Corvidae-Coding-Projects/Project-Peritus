//! Functionality of the exact command-candidate relation.

#[cfg(verus_only)]
use crate::{BudgetCommand, BudgetLedger, BudgetReceipt, BudgetReceiptKind};
use vstd::prelude::*;

mod lifecycle;
mod operations;
mod accounting;
mod allocation;
mod dispatch;
mod release;
mod reservation_updates;
mod fault;
mod finalization;
mod observation;

#[cfg(verus_only)]
pub(crate) use dispatch::candidate_step_is_functional;

verus! {

proof fn activation_candidates_equal(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    activation: crate::Activation,
    left_after: &BudgetLedger,
    left_receipt: BudgetReceipt,
    right_after: &BudgetLedger,
    right_receipt: BudgetReceipt,
)
    requires
        left_before.accounts@ == right_before.accounts@,
        left_before.reservations@ == right_before.reservations@,
        super::raw_accepted_step(
            left_before,
            BudgetCommand::Activate(activation),
            left_after,
            left_receipt,
        ),
        super::raw_accepted_step(
            right_before,
            BudgetCommand::Activate(activation),
            right_after,
            right_receipt,
        ),
    ensures
        super::commands::ledger_views_equal(left_after, right_after),
        super::commands::receipts_exactly_equal(left_receipt, right_receipt),
{
    reveal(super::raw_accepted_step);
    reveal(super::guards::accepted_command_guard);
    reveal(super::commands::activate_step);
    reveal(super::commands::bound_budget);
    reveal(super::commands::receipt_identity);
    reveal(super::reservations::activation_effect);
    reveal(super::reservations::activation_record_effect);
    reveal(super::reservations::unchanged_except);
    reveal(super::guards::reservation_at);
    assert(super::guards::accepted_command_guard(
        left_before,
        BudgetCommand::Activate(activation),
        left_receipt.spec_kind(),
    ));
    assert(super::guards::accepted_command_guard(
        right_before,
        BudgetCommand::Activate(activation),
        right_receipt.spec_kind(),
    ));
    assert(super::commands::activate_step(
        left_before, activation, left_after, left_receipt,
    ));
    assert(super::commands::activate_step(
        right_before, activation, right_after, right_receipt,
    ));
    let left_index = choose |index: int| #![auto]
        super::guards::reservation_at(
            left_before, activation.spec_reservation_id(), index,
        ) && super::guards::activation_binding(
            left_before.reservations[index], activation,
        ) && match left_receipt.spec_kind() {
            BudgetReceiptKind::Applied => {
                left_before.reservations[index].phase == crate::ReservationPhase::Held
            }
            BudgetReceiptKind::Idempotent => {
                left_before.reservations[index].phase == crate::ReservationPhase::Active
            }
            BudgetReceiptKind::OverrunFaulted => false,
        };
    let right_index = choose |index: int| #![auto]
        super::guards::reservation_at(
            right_before, activation.spec_reservation_id(), index,
        ) && super::guards::activation_binding(
            right_before.reservations[index], activation,
        ) && match right_receipt.spec_kind() {
            BudgetReceiptKind::Applied => {
                right_before.reservations[index].phase == crate::ReservationPhase::Held
            }
            BudgetReceiptKind::Idempotent => {
                right_before.reservations[index].phase == crate::ReservationPhase::Active
            }
            BudgetReceiptKind::OverrunFaulted => false,
        };
    assert(crate::model::ledger_well_formed(left_before));
    assert(crate::identity_model::reservation_ids_equal(
        left_before.reservations[left_index].request.spec_reservation_id(),
        left_before.reservations[right_index].request.spec_reservation_id(),
    ));
    crate::invariant::matching_reservations_are_unique(
        left_before, left_index, right_index,
    );
    assert(left_index == right_index);
    match (left_receipt.spec_kind(), right_receipt.spec_kind()) {
        (BudgetReceiptKind::Idempotent, BudgetReceiptKind::Idempotent) => {}
        (BudgetReceiptKind::Applied, BudgetReceiptKind::Applied) => {
            assert(super::reservations::activation_effect(
                left_before,
                left_after,
                activation.spec_reservation_id(),
                activation.spec_evidence_digest(),
            ));
            assert(super::reservations::activation_effect(
                right_before,
                right_after,
                activation.spec_reservation_id(),
                activation.spec_evidence_digest(),
            ));
            super::reservations::activation_effect_parts(
                left_before,
                left_after,
                activation.spec_reservation_id(),
                activation.spec_evidence_digest(),
            );
            super::reservations::activation_effect_parts(
                right_before,
                right_after,
                activation.spec_reservation_id(),
                activation.spec_evidence_digest(),
            );
            assert(super::reservations::unchanged_except(
                left_before, left_after, activation.spec_reservation_id(),
            ));
            assert(super::reservations::unchanged_except(
                right_before, right_after, activation.spec_reservation_id(),
            ));
            super::reservations::unchanged_except_has_witness(
                left_before, left_after, activation.spec_reservation_id(),
            );
            super::reservations::unchanged_except_has_witness(
                right_before, right_after, activation.spec_reservation_id(),
            );
            assert(exists |index: int| #![auto]
                0 <= index < left_before.reservations@.len()
                    && crate::identity_model::reservation_ids_equal(
                        left_before.reservations[index].request.spec_reservation_id(),
                        activation.spec_reservation_id(),
                    ) && left_after.reservations@ == left_before.reservations@.update(
                    index, left_after.reservations[index],
                ));
            assert(exists |index: int| #![auto]
                0 <= index < right_before.reservations@.len()
                    && crate::identity_model::reservation_ids_equal(
                        right_before.reservations[index].request.spec_reservation_id(),
                        activation.spec_reservation_id(),
                    ) && right_after.reservations@ == right_before.reservations@.update(
                    index, right_after.reservations[index],
                ));
            let left_updated = choose |index: int| #![auto]
                0 <= index < left_before.reservations@.len()
                    && crate::identity_model::reservation_ids_equal(
                        left_before.reservations[index].request.spec_reservation_id(),
                        activation.spec_reservation_id(),
                    ) && left_after.reservations@ == left_before.reservations@.update(
                    index, left_after.reservations[index],
                );
            let right_updated = choose |index: int| #![auto]
                0 <= index < right_before.reservations@.len()
                    && crate::identity_model::reservation_ids_equal(
                        right_before.reservations[index].request.spec_reservation_id(),
                        activation.spec_reservation_id(),
                    ) && right_after.reservations@ == right_before.reservations@.update(
                    index, right_after.reservations[index],
                );
            crate::invariant::matching_reservations_are_unique(
                left_before, left_index, left_updated,
            );
            crate::invariant::matching_reservations_are_unique(
                left_before, left_index, right_updated,
            );
            assert(left_updated == left_index);
            assert(right_updated == left_index);
            let left_effect = choose |index: int| #![auto]
                super::reservations::activation_record_effect(
                    left_before,
                    left_after,
                    activation.spec_reservation_id(),
                    activation.spec_evidence_digest(),
                    index,
                );
            let right_effect = choose |index: int| #![auto]
                super::reservations::activation_record_effect(
                    right_before,
                    right_after,
                    activation.spec_reservation_id(),
                    activation.spec_evidence_digest(),
                    index,
                );
            crate::invariant::matching_reservations_are_unique(
                left_before, left_index, left_effect,
            );
            crate::invariant::matching_reservations_are_unique(
                left_before, left_index, right_effect,
            );
            assert(left_effect == left_index);
            assert(right_effect == left_index);
            assert forall |index: int| #![auto]
                0 <= index < left_after.reservations@.len()
                    implies super::reservations::record_exactly_equal(
                        left_after.reservations[index],
                        right_after.reservations[index],
                    ) by {
                if index != left_index {
                    assert(left_after.reservations[index]
                        == left_before.reservations[index]);
                    assert(right_after.reservations[index]
                        == right_before.reservations[index]);
                }
            }
        }
        (BudgetReceiptKind::Idempotent, BudgetReceiptKind::Applied)
        | (BudgetReceiptKind::Applied, BudgetReceiptKind::Idempotent) => {
            assert(left_before.reservations[left_index]
                == right_before.reservations[left_index]);
            assert(false);
        }
        _ => assert(false),
    }
    assert(left_receipt.spec_operation() == crate::BudgetOperation::Activate);
    assert(right_receipt.spec_operation() == crate::BudgetOperation::Activate);
    assert(left_receipt.spec_kind() == right_receipt.spec_kind());
    let left_budget = choose |budget_id: peritus_types::BudgetId| #![auto]
        super::commands::bound_budget(
            left_before, activation.spec_reservation_id(), budget_id,
        ) && super::commands::receipt_identity(
            left_receipt,
            crate::BudgetOperation::Activate,
            budget_id,
            Some(activation.spec_reservation_id()),
        );
    let right_budget = choose |budget_id: peritus_types::BudgetId| #![auto]
        super::commands::bound_budget(
            right_before, activation.spec_reservation_id(), budget_id,
        ) && super::commands::receipt_identity(
            right_receipt,
            crate::BudgetOperation::Activate,
            budget_id,
            Some(activation.spec_reservation_id()),
        );
    let left_budget_index = choose |index: int| #![auto]
        0 <= index < left_before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                left_before.reservations[index].request.spec_reservation_id(),
                activation.spec_reservation_id(),
            )
            && crate::identity_model::budget_ids_equal(
                left_before.reservations[index].request.spec_budget_id(),
                left_budget,
            );
    let right_budget_index = choose |index: int| #![auto]
        0 <= index < right_before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                right_before.reservations[index].request.spec_reservation_id(),
                activation.spec_reservation_id(),
            )
            && crate::identity_model::budget_ids_equal(
                right_before.reservations[index].request.spec_budget_id(),
                right_budget,
            );
    crate::invariant::matching_reservations_are_unique(
        left_before, left_index, left_budget_index,
    );
    crate::invariant::matching_reservations_are_unique(
        left_before, left_index, right_budget_index,
    );
    assert(left_budget_index == left_index);
    assert(right_budget_index == left_index);
    assert(crate::identity_model::budget_ids_equal(left_budget, right_budget));
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
    assert(super::commands::receipts_exactly_equal(left_receipt, right_receipt));
}

} // verus!
