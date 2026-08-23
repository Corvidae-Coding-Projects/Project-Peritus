//! Functionality of child-allocation candidates.

#[cfg(verus_only)]
use crate::{BudgetCommand, BudgetLedger, BudgetReceipt, BudgetReceiptKind};
use vstd::prelude::*;

verus! {

pub(super) proof fn allocation_candidates_equal(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    request: crate::ChildBudgetRequest,
    left_after: &BudgetLedger,
    left_receipt: BudgetReceipt,
    right_after: &BudgetLedger,
    right_receipt: BudgetReceipt,
)
    requires
        left_before.accounts@ == right_before.accounts@,
        left_before.reservations@ == right_before.reservations@,
        crate::reachability::raw_accepted_step(
            left_before,
            BudgetCommand::AllocateChild(request),
            left_after,
            left_receipt,
        ),
        crate::reachability::raw_accepted_step(
            right_before,
            BudgetCommand::AllocateChild(request),
            right_after,
            right_receipt,
        ),
    ensures
        crate::reachability::commands::ledger_views_equal(left_after, right_after),
        crate::reachability::commands::receipts_exactly_equal(left_receipt, right_receipt),
{
    match (left_receipt.spec_kind(), right_receipt.spec_kind()) {
        (BudgetReceiptKind::Idempotent, BudgetReceiptKind::Idempotent) => {}
        (BudgetReceiptKind::Applied, BudgetReceiptKind::Applied) => {
            let left_parent = choose |parent: int| #![auto]
                0 <= parent < left_before.accounts@.len()
                    && crate::identity_model::budget_ids_equal(
                        left_before.accounts[parent].id,
                        request.spec_parent_id(),
                    )
                    && left_after.accounts@ == left_before.accounts@.update(
                        parent,
                        left_after.accounts[parent],
                    ).push(left_after.accounts[left_before.accounts@.len() as int]);
            let right_parent = choose |parent: int| #![auto]
                0 <= parent < right_before.accounts@.len()
                    && crate::identity_model::budget_ids_equal(
                        right_before.accounts[parent].id,
                        request.spec_parent_id(),
                    )
                    && right_after.accounts@ == right_before.accounts@.update(
                        parent,
                        right_after.accounts[parent],
                    ).push(right_after.accounts[right_before.accounts@.len() as int]);
            assert(crate::model::ledger_well_formed(left_before));
            assert(crate::identity_model::budget_ids_equal(
                left_before.accounts[left_parent].id,
                left_before.accounts[right_parent].id,
            ));
            crate::invariant::matching_accounts_are_unique(
                left_before, left_parent, right_parent,
            );
            assert(left_parent == right_parent);
            assert forall |index: int| #![auto]
                0 <= index < left_after.accounts@.len()
                    implies crate::reachability::accounts::account_exactly_equal(
                        left_after.accounts[index],
                        right_after.accounts[index],
                    ) by {
                if index == left_before.accounts@.len() {
                } else if index == left_parent {
                } else {
                    assert(left_after.accounts[index] == left_before.accounts[index]);
                    assert(right_after.accounts[index] == right_before.accounts[index]);
                }
            }
        }
        (BudgetReceiptKind::Idempotent, BudgetReceiptKind::Applied) => {
            let child = choose |index: int| #![auto]
                crate::reachability::guards::account_at(
                    left_before, request.spec_child_id(), index,
                );
            assert(crate::reachability::guards::account_at(
                right_before, request.spec_child_id(), child,
            ));
            assert(false);
        }
        (BudgetReceiptKind::Applied, BudgetReceiptKind::Idempotent) => {
            let child = choose |index: int| #![auto]
                crate::reachability::guards::account_at(
                    right_before, request.spec_child_id(), index,
                );
            assert(crate::reachability::guards::account_at(
                left_before, request.spec_child_id(), child,
            ));
            assert(false);
        }
        _ => assert(false),
    }
}

} // verus!
