//! Functionality of begin-operation candidates.

#[cfg(verus_only)]
use crate::{BudgetCommand, BudgetLedger, BudgetReceipt, BudgetReceiptKind};
use vstd::prelude::*;

verus! {

pub(super) proof fn begin_candidates_equal(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    request: crate::BudgetRequest,
    left_after: &BudgetLedger,
    left_receipt: BudgetReceipt,
    right_after: &BudgetLedger,
    right_receipt: BudgetReceipt,
)
    requires
        left_before.accounts@ == right_before.accounts@,
        left_before.reservations@ == right_before.reservations@,
        super::super::raw_accepted_step(
            left_before, BudgetCommand::Begin(request), left_after, left_receipt,
        ),
        super::super::raw_accepted_step(
            right_before, BudgetCommand::Begin(request), right_after, right_receipt,
        ),
    ensures
        super::super::commands::ledger_views_equal(left_after, right_after),
        super::super::commands::receipts_exactly_equal(left_receipt, right_receipt),
{
    reveal(super::super::raw_accepted_step);
    reveal(super::super::guards::accepted_command_guard);
    reveal(super::super::commands::begin_step);
    reveal(super::super::account_updates::begin_accounting);
    reveal(super::super::account_updates::lineage_charge);
    reveal(super::super::account_updates::operation_reserve);
    match (left_receipt.spec_kind(), right_receipt.spec_kind()) {
        (BudgetReceiptKind::Idempotent, BudgetReceiptKind::Idempotent) => {
        }
        (BudgetReceiptKind::Applied, BudgetReceiptKind::Applied) => {
            let left_charged = choose |charged_state: BudgetLedger| #![auto]
                super::super::account_updates::begin_accounting(
                    left_before,
                    left_after,
                    &charged_state,
                    request.spec_budget_id(),
                    request.spec_consume_now(),
                    request.spec_reserve(),
                );
            let right_charged = choose |charged_state: BudgetLedger| #![auto]
                super::super::account_updates::begin_accounting(
                    right_before,
                    right_after,
                    &charged_state,
                    request.spec_budget_id(),
                    request.spec_consume_now(),
                    request.spec_reserve(),
                );
            assert(crate::model::ledger_well_formed(left_before));
            super::accounting::well_formed_has_unique_account_ids(left_before);
            super::accounting::lineage_charge_fuel_functional(
                left_before.accounts@,
                right_before.accounts@,
                left_charged.accounts@,
                right_charged.accounts@,
                request.spec_budget_id(),
                request.spec_budget_id(),
                request.spec_consume_now(),
                request.spec_consume_now(),
                false,
                left_before.accounts@.len() as nat,
            );
            super::accounting::operation_reserve_functional(
                &left_charged,
                &right_charged,
                left_after,
                right_after,
                request.spec_budget_id(),
                request.spec_budget_id(),
                request.spec_reserve(),
            );
            assert(crate::identity_model::budget_ids_equal(
                left_after.root_id, right_after.root_id,
            ));
            assert(left_after.accounts@.len() == right_after.accounts@.len());
            assert(left_after.reservations@.len() == right_after.reservations@.len());
            assert forall |index: int| #![auto]
                0 <= index < left_after.reservations@.len()
                    implies super::super::reservations::record_exactly_equal(
                        left_after.reservations[index], right_after.reservations[index],
                    ) by {
                if index < left_before.reservations@.len() {
                    assert(left_after.reservations[index]
                        == left_before.reservations[index]);
                    assert(right_after.reservations[index]
                        == right_before.reservations[index]);
                }
            }
            assert forall |index: int| #![auto]
                0 <= index < left_after.accounts@.len()
                    implies super::super::accounts::account_exactly_equal(
                        left_after.accounts[index], right_after.accounts[index],
                    ) by {
            }
        }
        (BudgetReceiptKind::Idempotent, BudgetReceiptKind::Applied) => {
            let existing = choose |index: int| #![auto]
                super::super::guards::reservation_at(
                    left_before, request.spec_reservation_id(), index,
                );
            assert(super::super::guards::reservation_at(
                right_before, request.spec_reservation_id(), existing,
            ));
            assert(false);
        }
        (BudgetReceiptKind::Applied, BudgetReceiptKind::Idempotent) => {
            let existing = choose |index: int| #![auto]
                super::super::guards::reservation_at(
                    right_before, request.spec_reservation_id(), index,
                );
            assert(super::super::guards::reservation_at(
                left_before, request.spec_reservation_id(), existing,
            ));
            assert(false);
        }
        _ => assert(false),
    }
    assert(left_receipt.spec_kind() == right_receipt.spec_kind());
    assert(super::super::commands::receipts_exactly_equal(
        left_receipt, right_receipt,
    ));
}

} // verus!
