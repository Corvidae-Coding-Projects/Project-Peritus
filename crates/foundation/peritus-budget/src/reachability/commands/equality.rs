//! Exact stable equality and identity projections for commands and receipts.

#[cfg(verus_only)]
use crate::{BudgetLedger, BudgetOperation, BudgetReceipt};
#[cfg(verus_only)]
use peritus_types::{BudgetId, BudgetReservationId};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn ledgers_exactly_equal(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> bool {
    before.root_id == after.root_id
        && before.accounts@ == after.accounts@
        && before.reservations@ == after.reservations@
}

/// Equality of every stable mathematical ledger projection.
pub(crate) open spec fn ledger_views_equal(
    left: &BudgetLedger,
    right: &BudgetLedger,
) -> bool {
    crate::identity_model::budget_ids_equal(left.root_id, right.root_id)
        && left.accounts@.len() == right.accounts@.len()
        && left.reservations@.len() == right.reservations@.len()
        && (forall |index: int| #![auto]
            0 <= index < left.accounts@.len()
                ==> crate::reachability::accounts::account_exactly_equal(
                    left.accounts[index], right.accounts[index],
                ))
        && (forall |index: int| #![auto]
            0 <= index < left.reservations@.len()
                ==> crate::reachability::reservations::record_exactly_equal(
                    left.reservations[index], right.reservations[index],
                ))
}

pub(crate) proof fn ledger_equality_reflexive(ledger: &BudgetLedger)
    ensures ledgers_exactly_equal(ledger, ledger),
{
    assert(crate::identity_model::budget_ids_equal(ledger.root_id, ledger.root_id));
    assert forall |index: int| #![auto] 0 <= index < ledger.accounts@.len() implies
        crate::reachability::accounts::account_exactly_equal(
            ledger.accounts[index], ledger.accounts[index],
        ) by {}
    assert forall |index: int| #![auto] 0 <= index < ledger.reservations@.len() implies
        crate::reachability::reservations::record_exactly_equal(
            ledger.reservations[index], ledger.reservations[index],
        ) by {}
}

pub(crate) open spec fn accounts_exactly_equal(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> bool {
    before.root_id == after.root_id && before.accounts@ == after.accounts@
}

pub(crate) open spec fn reservations_exactly_equal(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> bool {
    before.reservations@ == after.reservations@
}

pub(crate) open spec fn receipt_identity(
    receipt: BudgetReceipt,
    operation: BudgetOperation,
    budget_id: BudgetId,
    reservation_id: Option<BudgetReservationId>,
) -> bool {
    receipt.spec_operation() == operation
        && crate::identity_model::budget_ids_equal(receipt.spec_budget_id(), budget_id)
        && crate::state::optional_reservation_ids_equal(
            receipt.spec_reservation_id(), reservation_id,
        )
}

pub(crate) open spec fn receipt_has_no_observation(receipt: BudgetReceipt) -> bool {
    receipt.spec_reported().is_none() && receipt.spec_evidence_digest().is_none()
}

pub(crate) open spec fn receipts_exactly_equal(
    left: BudgetReceipt,
    right: BudgetReceipt,
) -> bool {
    left.spec_operation() == right.spec_operation()
        && left.spec_kind() == right.spec_kind()
        && crate::identity_model::budget_ids_equal(
            left.spec_budget_id(), right.spec_budget_id(),
        )
        && crate::state::optional_reservation_ids_equal(
            left.spec_reservation_id(), right.spec_reservation_id(),
        )
        && left.spec_charged().spec_equal(right.spec_charged())
        && left.spec_released().spec_equal(right.spec_released())
        && crate::invariant::optional_amounts_equal(
            left.spec_reported(), right.spec_reported(),
        )
        && crate::invariant::optional_digests_equal(
            left.spec_evidence_digest(), right.spec_evidence_digest(),
        )
}

pub(crate) open spec fn bound_budget(
    ledger: &BudgetLedger,
    reservation_id: BudgetReservationId,
    budget_id: BudgetId,
) -> bool {
    exists |index: int| #![auto]
        0 <= index < ledger.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                ledger.reservations[index].request.spec_reservation_id(), reservation_id,
            )
            && crate::identity_model::budget_ids_equal(
                ledger.reservations[index].request.spec_budget_id(), budget_id,
            )
}

} // verus!
