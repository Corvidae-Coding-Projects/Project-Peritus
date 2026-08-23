//! Functionality of account lifecycle candidates.

#[cfg(verus_only)]
use crate::{BudgetCommand, BudgetLedger, BudgetReceipt, BudgetReceiptKind};
use vstd::prelude::*;

verus! {

pub(super) proof fn seal_candidates_equal(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    left_after: &BudgetLedger,
    left_receipt: BudgetReceipt,
    right_after: &BudgetLedger,
    right_receipt: BudgetReceipt,
)
    requires
        left_before.accounts@ == right_before.accounts@,
        left_before.reservations@ == right_before.reservations@,
        super::super::raw_accepted_step(
            left_before, BudgetCommand::Seal(budget_id), left_after, left_receipt,
        ),
        super::super::raw_accepted_step(
            right_before, BudgetCommand::Seal(budget_id), right_after, right_receipt,
        ),
    ensures
        super::super::commands::ledger_views_equal(left_after, right_after),
        super::super::commands::receipts_exactly_equal(left_receipt, right_receipt),
{
    reveal(super::super::raw_accepted_step);
    reveal(super::super::guards::accepted_command_guard);
    reveal(super::super::lifecycle_steps::lifecycle_step);
    reveal(super::super::lifecycle_steps::seal_step);
    reveal(super::super::accounts::account_phase_effect);
    reveal(super::super::accounts::account_phase_record_effect);
    let left_index = choose |index: int| #![auto]
        super::super::guards::account_at(left_before, budget_id, index)
            && match left_receipt.spec_kind() {
                BudgetReceiptKind::Applied => {
                    left_before.accounts[index].phase == crate::BudgetAccountPhase::Open
                }
                BudgetReceiptKind::Idempotent => {
                    left_before.accounts[index].phase != crate::BudgetAccountPhase::Open
                }
                BudgetReceiptKind::OverrunFaulted => false,
            };
    let right_index = choose |index: int| #![auto]
        super::super::guards::account_at(right_before, budget_id, index)
            && match right_receipt.spec_kind() {
                BudgetReceiptKind::Applied => {
                    right_before.accounts[index].phase == crate::BudgetAccountPhase::Open
                }
                BudgetReceiptKind::Idempotent => {
                    right_before.accounts[index].phase != crate::BudgetAccountPhase::Open
                }
                BudgetReceiptKind::OverrunFaulted => false,
            };
    assert(crate::model::ledger_well_formed(left_before));
    crate::invariant::matching_accounts_are_unique(left_before, left_index, right_index);
    assert(left_index == right_index);
    match (left_receipt.spec_kind(), right_receipt.spec_kind()) {
        (BudgetReceiptKind::Idempotent, BudgetReceiptKind::Idempotent) => {}
        (BudgetReceiptKind::Applied, BudgetReceiptKind::Applied) => {
            assert(super::super::accounts::account_phase_effect(
                left_before,
                left_after,
                budget_id,
                crate::BudgetAccountPhase::Draining,
            ));
            assert(super::super::accounts::account_phase_effect(
                right_before,
                right_after,
                budget_id,
                crate::BudgetAccountPhase::Draining,
            ));
            super::super::accounts::account_phase_effect_parts(
                left_before,
                left_after,
                budget_id,
                crate::BudgetAccountPhase::Draining,
            );
            super::super::accounts::account_phase_effect_parts(
                right_before,
                right_after,
                budget_id,
                crate::BudgetAccountPhase::Draining,
            );
            let left_updated = choose |index: int| #![auto]
                super::super::accounts::account_phase_record_effect(
                    left_before,
                    left_after,
                    budget_id,
                    crate::BudgetAccountPhase::Draining,
                    index,
                );
            let right_updated = choose |index: int| #![auto]
                super::super::accounts::account_phase_record_effect(
                    right_before,
                    right_after,
                    budget_id,
                    crate::BudgetAccountPhase::Draining,
                    index,
                );
            crate::invariant::matching_accounts_are_unique(
                left_before, left_index, left_updated,
            );
            crate::invariant::matching_accounts_are_unique(
                left_before, left_index, right_updated,
            );
            assert(left_updated == left_index);
            assert(right_updated == left_index);
            assert forall |index: int| #![auto]
                0 <= index < left_after.accounts@.len()
                    implies super::super::accounts::account_exactly_equal(
                        left_after.accounts[index], right_after.accounts[index],
                    ) by {
                if index != left_index {
                    assert(left_after.accounts[index] == left_before.accounts[index]);
                    assert(right_after.accounts[index] == right_before.accounts[index]);
                }
            }
        }
        (BudgetReceiptKind::Idempotent, BudgetReceiptKind::Applied)
        | (BudgetReceiptKind::Applied, BudgetReceiptKind::Idempotent) => {
            assert(left_before.accounts[left_index] == right_before.accounts[left_index]);
            assert(false);
        }
        _ => assert(false),
    }
    assert(left_receipt.spec_operation() == crate::BudgetOperation::Seal);
    assert(right_receipt.spec_operation() == crate::BudgetOperation::Seal);
    assert(left_receipt.spec_kind() == right_receipt.spec_kind());
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
}

pub(super) proof fn close_candidates_equal(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    left_after: &BudgetLedger,
    left_receipt: BudgetReceipt,
    right_after: &BudgetLedger,
    right_receipt: BudgetReceipt,
)
    requires
        left_before.accounts@ == right_before.accounts@,
        left_before.reservations@ == right_before.reservations@,
        super::super::raw_accepted_step(
            left_before, BudgetCommand::Close(budget_id), left_after, left_receipt,
        ),
        super::super::raw_accepted_step(
            right_before, BudgetCommand::Close(budget_id), right_after, right_receipt,
        ),
    ensures
        super::super::commands::ledger_views_equal(left_after, right_after),
        super::super::commands::receipts_exactly_equal(left_receipt, right_receipt),
{
    reveal(super::super::raw_accepted_step);
    reveal(super::super::guards::accepted_command_guard);
    reveal(super::super::lifecycle_steps::lifecycle_step);
    reveal(super::super::lifecycle_steps::close_step);
    reveal(super::super::allocation::close_target_record_effect);
    reveal(super::super::allocation::close_parent_record_effect);
    let left_index = choose |index: int| #![auto]
        super::super::guards::account_at(left_before, budget_id, index)
            && match left_receipt.spec_kind() {
                BudgetReceiptKind::Idempotent => {
                    left_before.accounts[index].phase == crate::BudgetAccountPhase::Closed
                }
                BudgetReceiptKind::Applied => {
                    (left_before.accounts[index].phase == crate::BudgetAccountPhase::Draining
                        || left_before.accounts[index].phase == crate::BudgetAccountPhase::Faulted)
                        && crate::invariant::budget_has_no_live_work(left_before, budget_id)
                }
                BudgetReceiptKind::OverrunFaulted => false,
            };
    let right_index = choose |index: int| #![auto]
        super::super::guards::account_at(right_before, budget_id, index)
            && match right_receipt.spec_kind() {
                BudgetReceiptKind::Idempotent => {
                    right_before.accounts[index].phase == crate::BudgetAccountPhase::Closed
                }
                BudgetReceiptKind::Applied => {
                    (right_before.accounts[index].phase == crate::BudgetAccountPhase::Draining
                        || right_before.accounts[index].phase == crate::BudgetAccountPhase::Faulted)
                        && crate::invariant::budget_has_no_live_work(right_before, budget_id)
                }
                BudgetReceiptKind::OverrunFaulted => false,
            };
    assert(crate::model::ledger_well_formed(left_before));
    crate::invariant::matching_accounts_are_unique(left_before, left_index, right_index);
    assert(left_index == right_index);
    match (left_receipt.spec_kind(), right_receipt.spec_kind()) {
        (BudgetReceiptKind::Idempotent, BudgetReceiptKind::Idempotent) => {}
        (BudgetReceiptKind::Applied, BudgetReceiptKind::Applied) => {
            assert(super::super::allocation::close_account_effect(
                left_before, left_after, budget_id, left_receipt.spec_released(),
            ));
            assert(super::super::allocation::close_account_effect(
                right_before, right_after, budget_id, right_receipt.spec_released(),
            ));
            super::super::allocation::close_account_effect_parts(
                left_before, left_after, budget_id, left_receipt.spec_released(),
            );
            super::super::allocation::close_account_effect_parts(
                right_before, right_after, budget_id, right_receipt.spec_released(),
            );
            let left_target = choose |target: int| #![auto]
                super::super::allocation::close_target_record_effect(
                    left_before,
                    left_after,
                    budget_id,
                    left_receipt.spec_released(),
                    target,
                );
            let right_target = choose |target: int| #![auto]
                super::super::allocation::close_target_record_effect(
                    right_before,
                    right_after,
                    budget_id,
                    right_receipt.spec_released(),
                    target,
                );
            crate::invariant::matching_accounts_are_unique(
                left_before, left_index, left_target,
            );
            crate::invariant::matching_accounts_are_unique(
                left_before, left_index, right_target,
            );
            assert(left_target == left_index);
            assert(right_target == left_index);
            assert(left_receipt.spec_released().spec_equal(
                right_receipt.spec_released(),
            ));
            let left_shape = choose |target: int| #![auto]
                super::super::allocation::close_target_record_effect(
                    left_before,
                    left_after,
                    budget_id,
                    left_receipt.spec_released(),
                    target,
                )
                    && match left_before.accounts[target].parent_id {
                        None => left_after.accounts@ == left_before.accounts@.update(
                            target, left_after.accounts[target],
                        ),
                        Some(_) => exists |parent: int| #![auto]
                            0 <= parent < left_before.accounts@.len()
                                && crate::identity_model::parent_matches(
                                    left_before.accounts[target].parent_id,
                                    left_before.accounts[parent].id,
                                )
                                && super::super::allocation::close_parent_record_effect(
                                    left_before,
                                    left_after,
                                    parent,
                                    left_receipt.spec_released(),
                                )
                                && left_after.accounts@ == left_before.accounts@.update(
                                    parent, left_after.accounts[parent],
                                ).update(target, left_after.accounts[target]),
                    };
            let right_shape = choose |target: int| #![auto]
                super::super::allocation::close_target_record_effect(
                    right_before,
                    right_after,
                    budget_id,
                    right_receipt.spec_released(),
                    target,
                )
                    && match right_before.accounts[target].parent_id {
                        None => right_after.accounts@ == right_before.accounts@.update(
                            target, right_after.accounts[target],
                        ),
                        Some(_) => exists |parent: int| #![auto]
                            0 <= parent < right_before.accounts@.len()
                                && crate::identity_model::parent_matches(
                                    right_before.accounts[target].parent_id,
                                    right_before.accounts[parent].id,
                                )
                                && super::super::allocation::close_parent_record_effect(
                                    right_before,
                                    right_after,
                                    parent,
                                    right_receipt.spec_released(),
                                )
                                && right_after.accounts@ == right_before.accounts@.update(
                                    parent, right_after.accounts[parent],
                                ).update(target, right_after.accounts[target]),
                    };
            crate::invariant::matching_accounts_are_unique(
                left_before, left_index, left_shape,
            );
            crate::invariant::matching_accounts_are_unique(
                left_before, left_index, right_shape,
            );
            assert(left_shape == left_index);
            assert(right_shape == left_index);
            match left_before.accounts[left_index].parent_id {
                None => {
                    assert forall |index: int| #![auto]
                        0 <= index < left_after.accounts@.len()
                            implies super::super::accounts::account_exactly_equal(
                                left_after.accounts[index], right_after.accounts[index],
                            ) by {
                        if index != left_index {
                            assert(left_after.accounts[index] == left_before.accounts[index]);
                            assert(right_after.accounts[index] == right_before.accounts[index]);
                        }
                    }
                }
                Some(_) => {
                    let left_parent = choose |parent: int| #![auto]
                        0 <= parent < left_before.accounts@.len()
                            && crate::identity_model::parent_matches(
                                left_before.accounts[left_index].parent_id,
                                left_before.accounts[parent].id,
                            )
                            && super::super::allocation::close_parent_record_effect(
                                left_before,
                                left_after,
                                parent,
                                left_receipt.spec_released(),
                            )
                            && left_after.accounts@ == left_before.accounts@.update(
                                parent, left_after.accounts[parent],
                            ).update(left_index, left_after.accounts[left_index]);
                    let right_parent = choose |parent: int| #![auto]
                        0 <= parent < right_before.accounts@.len()
                            && crate::identity_model::parent_matches(
                                right_before.accounts[left_index].parent_id,
                                right_before.accounts[parent].id,
                            )
                            && super::super::allocation::close_parent_record_effect(
                                right_before,
                                right_after,
                                parent,
                                right_receipt.spec_released(),
                            )
                            && right_after.accounts@ == right_before.accounts@.update(
                                parent, right_after.accounts[parent],
                            ).update(left_index, right_after.accounts[left_index]);
                    crate::invariant::matching_accounts_are_unique(
                        left_before, left_parent, right_parent,
                    );
                    assert(left_parent == right_parent);
                    assert forall |index: int| #![auto]
                        0 <= index < left_after.accounts@.len()
                            implies super::super::accounts::account_exactly_equal(
                                left_after.accounts[index], right_after.accounts[index],
                            ) by {
                        if index != left_index && index != left_parent {
                            assert(left_after.accounts[index] == left_before.accounts[index]);
                            assert(right_after.accounts[index] == right_before.accounts[index]);
                        }
                    }
                }
            }
        }
        (BudgetReceiptKind::Idempotent, BudgetReceiptKind::Applied)
        | (BudgetReceiptKind::Applied, BudgetReceiptKind::Idempotent) => {
            assert(left_before.accounts[left_index] == right_before.accounts[left_index]);
            assert(false);
        }
        _ => assert(false),
    }
    assert(left_receipt.spec_operation() == crate::BudgetOperation::Close);
    assert(right_receipt.spec_operation() == crate::BudgetOperation::Close);
    assert(left_receipt.spec_kind() == right_receipt.spec_kind());
    assert(crate::identity_model::budget_ids_equal(
        left_receipt.spec_budget_id(), right_receipt.spec_budget_id(),
    ));
    assert(crate::state::optional_reservation_ids_equal(
        left_receipt.spec_reservation_id(), right_receipt.spec_reservation_id(),
    ));
    assert(left_receipt.spec_charged().spec_equal(right_receipt.spec_charged()));
    assert(crate::invariant::optional_amounts_equal(
        left_receipt.spec_reported(), right_receipt.spec_reported(),
    ));
    assert(crate::invariant::optional_digests_equal(
        left_receipt.spec_evidence_digest(), right_receipt.spec_evidence_digest(),
    ));
}

} // verus!
